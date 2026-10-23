const communityRules = require('../../utils/community-rules');

Page({
  data: {
    ...communityRules,
    version: communityRules.version,
  },

  onLoad() {
    wx.setNavigationBarTitle({ title: '社区规则' });
  },
});
