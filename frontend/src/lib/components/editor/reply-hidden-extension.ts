import { Node } from '@tiptap/core';

/**
 * Tiptap 扩展：回复可见内嵌块 (ReplyHidden)。
 *
 * 语法与后端 CommonMark 指令对齐：
 * - Markdown 序列化 / 反序列化：
 *   :::reply
 *   此处为回复后可见的内容...
 *   :::
 *
 * 所见即所得 (WYSIWYG) 呈现：
 * - 渲染为专用的回复可见卡片（含带锁角标与虚线高亮边框），
 * - 内部是可完全编辑的 block+ 内容（支持多段落、格式化等）。
 */
export const ReplyHidden = Node.create({
  name: 'replyHidden',
  group: 'block',
  content: 'block+',
  defining: true,
  isolating: true,

  parseHTML() {
    return [
      {
        tag: 'div[data-reply-hidden]',
        contentElement: (node) => {
          return (node as HTMLElement).querySelector('.topic-restricted-editor-content') || (node as HTMLElement);
        }
      }
    ];
  },

  renderHTML() {
    return [
      'div',
      {
        'data-reply-hidden': 'true',
        class: 'topic-restricted-editor-box'
      },
      [
        'div',
        {
          class: 'topic-restricted-editor-banner',
          contenteditable: 'false'
        },
        [
          'span',
          { class: 'topic-restricted-editor-badge' },
          '🔒 回复可见内容'
        ],
        [
          'span',
          { class: 'topic-restricted-editor-hint' },
          '仅在本帖参与回复的用户可见'
        ]
      ],
      ['div', { class: 'topic-restricted-editor-content' }, 0]
    ];
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          state.write(':::reply\n');
          state.renderContent(node);
          state.ensureNewLine();
          state.write(':::\n');
          state.closeBlock(node);
        },
        parse: {
          setup(md: any) {
            // 注册 markdown-it block ruler，在 fence 之前捕获 :::reply ... :::
            function replyBlockRule(state: any, startLine: number, endLine: number, silent: boolean) {
              const pos = state.bMarks[startLine] + state.tShift[startLine];
              const max = state.eMarks[startLine];
              if (state.sCount[startLine] - state.blkIndent >= 4) return false;
              const lineText = state.src.slice(pos, max).trim();
              if (!lineText.startsWith(':::reply') && !lineText.startsWith(':::hide')) {
                return false;
              }
              if (silent) return true;

              let nextLine = startLine;
              let haveClose = false;
              for (;;) {
                nextLine++;
                if (nextLine >= endLine) break;
                const p = state.bMarks[nextLine] + state.tShift[nextLine];
                const m = state.eMarks[nextLine];
                if (state.sCount[nextLine] < state.blkIndent) break;
                const text = state.src.slice(p, m).trim();
                if (text.startsWith(':::')) {
                  haveClose = true;
                  break;
                }
              }

              let token = state.push('reply_hidden_open', 'div', 1);
              token.attrs = [['data-reply-hidden', 'true']];
              token.block = true;
              token.map = [startLine, nextLine];

              state.md.block.tokenize(state, startLine + 1, nextLine);

              token = state.push('reply_hidden_close', 'div', -1);
              token.block = true;

              state.line = haveClose ? nextLine + 1 : nextLine;
              return true;
            }

            md.block.ruler.before('fence', 'reply_hidden', replyBlockRule);
          }
        }
      }
    };
  }
});
