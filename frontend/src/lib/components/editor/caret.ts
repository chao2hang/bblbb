/**
 * 获取 textarea 中指定字符下标 (position) 对应的 (top, left, lineHeight) 相对坐标
 * 基于 mirror div 精确度量，广泛用于 mention 浮窗、代码补全等场景。
 */

const PROPERTIES = [
  'direction',
  'boxSizing',
  'overflowX',
  'overflowY',
  'borderTopWidth',
  'borderRightWidth',
  'borderBottomWidth',
  'borderLeftWidth',
  'borderStyle',
  'paddingTop',
  'paddingRight',
  'paddingBottom',
  'paddingLeft',
  'fontStyle',
  'fontVariant',
  'fontWeight',
  'fontStretch',
  'fontSize',
  'fontSizeAdjust',
  'lineHeight',
  'fontFamily',
  'textAlign',
  'textTransform',
  'textIndent',
  'textDecoration',
  'letterSpacing',
  'wordSpacing',
  'tabSize',
  'whiteSpace',
  'wordBreak',
  'overflowWrap'
] as const;

export interface CaretCoordinates {
  top: number;
  left: number;
  lineHeight: number;
}

export function getCaretCoordinates(
  element: HTMLTextAreaElement,
  position: number
): CaretCoordinates {
  if (typeof window === 'undefined' || typeof document === 'undefined') {
    return { top: 0, left: 0, lineHeight: 20 };
  }

  const computed = window.getComputedStyle(element);

  const div = document.createElement('div');
  div.id = 'input-textarea-caret-position-mirror-div';
  document.body.appendChild(div);

  const style = div.style;
  style.whiteSpace = 'pre-wrap';
  style.wordWrap = 'break-word';
  style.position = 'absolute';
  style.visibility = 'hidden';
  style.top = '0';
  style.left = '-9999px';

  for (const prop of PROPERTIES) {
    (style as Record<string, any>)[prop] = (computed as Record<string, any>)[prop];
  }

  if (computed.boxSizing === 'border-box') {
    style.width = `${element.offsetWidth || element.clientWidth || 300}px`;
  } else {
    style.width = `${element.clientWidth || 300}px`;
  }

  div.textContent = element.value.substring(0, position);

  const span = document.createElement('span');
  span.textContent = element.value.substring(position) || '.';
  div.appendChild(span);

  const borderTop = parseInt(computed.borderTopWidth, 10) || 0;
  const borderLeft = parseInt(computed.borderLeftWidth, 10) || 0;
  const lineHeight = parseInt(computed.lineHeight, 10) || (parseInt(computed.fontSize, 10) || 14) * 1.5;

  const top = span.offsetTop + borderTop;
  const left = span.offsetLeft + borderLeft;

  document.body.removeChild(div);

  return { top, left, lineHeight };
}
