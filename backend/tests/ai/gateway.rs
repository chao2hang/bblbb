//! M09-GATEWAY 集成测试：出站策略（HTTPS/host/端口/IP/重定向/大小）、脱敏规则、
//! 预算熔断（纯函数，无网络调用）。

use bblbb_backend::ai::gateway::{is_private_ip, BudgetCounter};
use bblbb_backend::ai::{EgressPolicy, GatewayError, RedactionMode, Redactor};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

fn policy_with_host() -> EgressPolicy {
    EgressPolicy {
        host_allowlist: vec!["api.mock.example".into()],
        ..Default::default()
    }
}

#[test]
fn egress_rejects_insecure_scheme() {
    let p = policy_with_host();
    assert!(matches!(
        p.validate_endpoint("http://api.mock.example/v1/chat"),
        Err(GatewayError::InsecureScheme(_))
    ));
}

#[test]
fn egress_rejects_host_outside_allowlist() {
    let p = policy_with_host();
    assert!(matches!(
        p.validate_endpoint("https://evil.example.com/v1"),
        Err(GatewayError::HostNotAllowed(_))
    ));
}

#[test]
fn egress_rejects_non_443_port() {
    let p = policy_with_host();
    assert!(matches!(
        p.validate_endpoint("https://api.mock.example:8443/v1"),
        Err(GatewayError::PortNotAllowed(8443))
    ));
}

#[test]
fn egress_rejects_private_ip_literal() {
    // IP 字面量在解析前直接做私网检查（DNS rebinding 解析前防线）。
    let p = EgressPolicy::default();
    assert!(matches!(
        p.validate_endpoint("https://10.0.0.5/v1"),
        Err(GatewayError::PrivateIp(_))
    ));
    assert!(matches!(
        p.validate_endpoint("https://[::1]/v1"),
        Err(GatewayError::PrivateIp(_))
    ));
}

#[test]
fn egress_allows_https_allowlisted_host() {
    let p = policy_with_host();
    assert_eq!(
        p.validate_endpoint("https://api.mock.example/v1/chat")
            .unwrap(),
        "api.mock.example"
    );
    // 显式 443 端口合法。
    assert!(p
        .validate_endpoint("https://api.mock.example:443/v1")
        .is_ok());
}

#[test]
fn egress_userinfo_and_malformed_rejected() {
    let p = policy_with_host();
    assert!(matches!(
        p.validate_endpoint("https://u:p@api.mock.example/v1"),
        Err(GatewayError::Invalid(_))
    ));
    assert!(matches!(
        p.validate_endpoint("not-a-url"),
        Err(GatewayError::Invalid(_))
    ));
    // 空 host（仅斜杠）也拒绝。
    assert!(p.validate_endpoint("https:///v1").is_err());
}

#[test]
fn egress_redirect_and_size_limits() {
    let p = EgressPolicy::default();
    assert!(p.check_redirect(0).is_ok());
    assert!(p.check_redirect(2).is_ok());
    assert!(matches!(
        p.check_redirect(3),
        Err(GatewayError::TooManyRedirects)
    ));
    assert!(p.check_response_size(p.max_response_bytes).is_ok());
    assert!(matches!(
        p.check_response_size(p.max_response_bytes + 1),
        Err(GatewayError::ResponseTooLarge(_))
    ));
}

#[test]
fn gateway_error_codes_are_stable() {
    assert_eq!(
        GatewayError::InsecureScheme("x".into()).code(),
        "ai_gateway_insecure_scheme"
    );
    assert_eq!(
        GatewayError::HostNotAllowed("x".into()).code(),
        "ai_gateway_host_not_allowed"
    );
    assert_eq!(
        GatewayError::PortNotAllowed(80).code(),
        "ai_gateway_port_not_allowed"
    );
    assert_eq!(
        GatewayError::PrivateIp("x".into()).code(),
        "ai_gateway_private_ip"
    );
    assert_eq!(
        GatewayError::Timeout("x".into()).code(),
        "ai_gateway_timeout"
    );
    assert_eq!(
        GatewayError::ResponseTooLarge(1).code(),
        "ai_gateway_response_too_large"
    );
    assert_eq!(
        GatewayError::BudgetExceeded("x".into()).code(),
        "ai_budget_exceeded"
    );
    assert_eq!(
        GatewayError::SecretNotConfigured.code(),
        "ai_provider_secret_not_configured"
    );
}

#[test]
fn private_ip_detection_covers_v4_v6() {
    assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
    assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)))); // CGNAT
    assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(198, 18, 0, 1)))); // benchmark
    assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::new(
        0xfd00, 0, 0, 0, 0, 0, 0, 1
    )))); // ULA
    assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::new(
        0x2001, 0x0db8, 0, 0, 0, 0, 0, 1
    )))); // 文档
    assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    assert!(!is_private_ip(&IpAddr::V6(Ipv6Addr::new(
        0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888
    ))));
}

#[test]
fn redactor_blocks_disabled_and_metadata_only() {
    assert_eq!(Redactor::redact("secret body", RedactionMode::Disabled), "");
    assert_eq!(
        Redactor::redact("secret body", RedactionMode::MetadataOnly),
        ""
    );
}

#[test]
fn redactor_strips_emails_in_redacted_mode() {
    let out = Redactor::redact(
        "contact me at alice@example.com or bob+tag@sub.example.co.uk thanks",
        RedactionMode::Redacted,
    );
    assert!(!out.contains("alice@example.com"));
    assert!(!out.contains("bob+tag@sub.example.co.uk"));
    assert!(out.contains("[email removed]"));
}

#[test]
fn budget_reserve_release_and_limits() {
    let mut b = BudgetCounter::new(1000, 2);
    assert!(b.reserve(400).is_ok());
    assert!(b.reserve(400).is_ok());
    // 并发超限。
    assert!(matches!(b.reserve(1), Err(GatewayError::BudgetExceeded(_))));
    b.release(300);
    b.release(400);
    // 预算耗尽：used=700 + 400 > 1000。
    assert!(matches!(
        b.reserve(400),
        Err(GatewayError::BudgetExceeded(_))
    ));
    assert!(b.reserve(300).is_ok());
}

#[test]
fn budget_circuit_breaker() {
    let mut b = BudgetCounter::new(100_000, 10);
    b.note_failure(5, 5);
    assert!(b.circuit_open);
    assert!(matches!(b.reserve(1), Err(GatewayError::BudgetExceeded(_))));
    b.close_circuit();
    assert!(!b.circuit_open);
    assert!(b.reserve(1).is_ok());
}
