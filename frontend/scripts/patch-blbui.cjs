const fs = require('fs');
const path = require('path');

const coreIndexPath = path.resolve(__dirname, '../node_modules/@chaos_team/blbui-core/dist/index.js');
if (fs.existsSync(coreIndexPath)) {
  let content = fs.readFileSync(coreIndexPath, 'utf8');
  if (!content.includes('from "./register.js"')) {
    content = 'export { adminElementTags, registerAdminElements, whenAdminElementsDefined } from "./register.js";\n' + content;
    content = content.replace(/adminElementTags,\s*/g, '');
    content = content.replace(/registerAdminElements,\s*/g, '');
    content = content.replace(/whenAdminElementsDefined\s*/g, '');
    fs.writeFileSync(coreIndexPath, content, 'utf8');
    console.log('Successfully patched blbui-core/dist/index.js');
  }
}
