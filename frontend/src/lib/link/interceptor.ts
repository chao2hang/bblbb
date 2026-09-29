/**
 * 链接跳转拦截器与安全校验模块
 *
 * 用于拦截全站所有跳出本站点的外链，重定向至跳转确认页面（参考 CNB / 外部跳转安全拦截设计）。
 */

/**
 * 判断给定的 href 是否属于跳出本站点的外链
 *
 * @param href 待检测的链接地址
 * @param currentOrigin 当前站点的 origin，例如 http://localhost:5173 或 https://example.com
 */
export function isExternalUrl(href: string | null | undefined, currentOrigin: string): boolean {
  if (!href) return false;
  const trimmed = href.trim();
  if (!trimmed) return false;


  try {
    // URL parsing normalizes browser-special backslashes and protocol-relative URLs,
    // avoiding prefix-based misclassification such as `/\\evil.example`.
    const url = new URL(trimmed, currentOrigin);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return false;
    return url.origin !== currentOrigin;
  } catch {
    return false;
  }
}

/**
 * 构造跳转确认页面的完整相对路径
 */
export function buildJumpUrl(targetUrl: string): string {
  return `/link?target=${encodeURIComponent(targetUrl)}`;
}

export interface ValidatedJumpTarget {
  valid: boolean;
  url: string;
  reason?: 'missing' | 'unsafe_scheme' | 'malformed';
}

/**
 * 严格校验跳转目标 URL 是否安全合法（仅支持 HTTP/HTTPS）
 */
export function validateJumpTarget(target: string | null | undefined): ValidatedJumpTarget {
  if (!target || !target.trim()) {
    return { valid: false, url: '', reason: 'missing' };
  }

  const trimmed = target.trim();
  const isProtocolRelative = trimmed.startsWith('//');
  const hasScheme = /^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(trimmed);
  if (!isProtocolRelative && !hasScheme) {
    return { valid: false, url: trimmed, reason: 'malformed' };
  }
  try {
    // Treat protocol-relative links consistently with browser navigation.
    const parsed = new URL(trimmed, 'https://link-target.invalid');
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
      return { valid: false, url: trimmed, reason: 'unsafe_scheme' };
    }
    return { valid: true, url: parsed.href };
  } catch {
    return { valid: false, url: trimmed, reason: 'malformed' };
  }
}

/**
 * 设置全局外链点击拦截器
 * 拦截文档内所有 a 标签发起的跳出站点的点击，重定向到跳转确认界面。
 *
 * @param doc 可选的 Document 对象（便于测试注入）
 * @param win 可选的 Window 对象（便于测试注入）
 * @returns 清理函数
 */
export function setupExternalLinkInterceptor(
  doc: Document = document,
  win: Window = window
): () => void {
  function handleGlobalClick(event: MouseEvent) {
    // Ignore canceled events, non-left clicks, and auxiliary clicks other than middle-click.
    if (event.defaultPrevented) return;
    const isMiddleClick = event.type === 'auxclick' && event.button === 1;
    const isPrimaryClick = event.type === 'click' && event.button === 0;
    if (!isMiddleClick && !isPrimaryClick) return;

    const target = event.target as Element | null;
    const anchor = target?.closest?.('a');
    if (!anchor) return;

    // 如果标记了直接跳转（如跳转确认界面的「继续访问」按钮），放行
    if (
      anchor.getAttribute('data-direct-jump') === 'true' ||
      anchor.classList.contains('link-confirm-proceed')
    ) {
      return;
    }

    const href = anchor.getAttribute('href');
    if (!href) return;

    const currentOrigin = win.location.origin;
    if (isExternalUrl(href, currentOrigin)) {
      event.preventDefault();
      event.stopPropagation();

      const jumpUrl = buildJumpUrl(href);
      const isNewTab =
        anchor.getAttribute('target') === '_blank' ||
        event.ctrlKey ||
        event.metaKey ||
        isMiddleClick;

      if (isNewTab) {
        win.open(jumpUrl, '_blank', 'noopener,noreferrer');
      } else {
        win.location.href = jumpUrl;
      }
    }
  }

  doc.addEventListener('click', handleGlobalClick, { capture: true });
  doc.addEventListener('auxclick', handleGlobalClick, { capture: true });

  return () => {
    doc.removeEventListener('click', handleGlobalClick, { capture: true });
    doc.removeEventListener('auxclick', handleGlobalClick, { capture: true });
  };
}
