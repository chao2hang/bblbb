//! M07-SHOP：内部商城、权益与补偿。
//!
//! - `service.rs`：商品/订单/权益/装备/presentation 服务层与 admin 操作
//! - 规则（SCHEMA.md §14 / INTERNAL-MARKETPLACE.md）：服务端重算价格/库存/
//!   等级/销售窗口/限购；购买同事务锁库存+扣账本+写订单+发权益+审计+Outbox；
//!   数字装扮默认不可退款；异常用补偿流水。
//! - Token 白名单：商品 icon_token 与 presentation_tokens_json 只允许
//!   注册的安全 Token，拒绝任意 CSS/HTML/JS/URL/SVG（M07-SHOP-SCHEMA-03）。

pub mod service;

pub use service::{
    buy_product, create_product, disable_product, equip, get_order, get_presentation, get_product,
    list_admin_orders, list_admin_products, list_my_entitlements, list_products, publish_product,
    refund_order, unequip, update_product, validate_tokens, ShopError,
};
