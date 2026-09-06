/**
 * UUID v4 生成（小程序环境无 crypto.randomUUID 的可靠实现，自实现）。
 * 用于幂等键 client_request_id（后端要求 16–200 字符）。
 */
function uuidv4() {
  // 16 进制随机；以 8-4-4-4-12 分段
  let hex = '';
  for (let i = 0; i < 32; i += 1) {
    hex += Math.floor(Math.random() * 16).toString(16);
  }
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-4${hex.slice(13, 16)}-${(
    8 + Math.floor(Math.random() * 4)
  )}${hex.slice(17, 20)}-${hex.slice(20)}`;
}

module.exports = {
  uuidv4,
};
