// MyDesk 入口：MyDesk 构建（内置配置含 "mydesk": "Y"）时，桌面主页换成新界面。
// 未登录 → 被控端视图；运维账号登录 → 运维台。远程窗口、设置、托盘等沿用官方实现。
import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:get/get.dart';

import 'controlled_view.dart';
import 'operator_view.dart';

bool? _isMyDesk;

/// 是否为 MyDesk 构建。
bool isMyDesk() => _isMyDesk ??= bind.mainGetHardOption(key: 'mydesk') == 'Y';

class MdHome extends StatelessWidget {
  /// 官方的安装、系统授权、升级提示卡（被控端视图顶部展示）。
  final Widget? systemCards;
  const MdHome({super.key, this.systemCards});

  @override
  Widget build(BuildContext context) {
    return Obx(() => gFFI.userModel.userName.value.isNotEmpty
        ? const MdOperatorView()
        : MdControlledView(systemCards: systemCards));
  }
}
