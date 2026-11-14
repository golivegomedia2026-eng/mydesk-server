// MyDesk 被控端主页：大号 ID、服务状态、当前协助状态、一键求助、运维登录入口。
// 不显示密码（由服务端托管），也不提供服务器设置。
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/common/widgets/login.dart';
import 'package:flutter_hbb/desktop/pages/connection_page.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:flutter_hbb/models/server_model.dart';
import 'package:provider/provider.dart';

import '../utils/http_service.dart' as http;
import 'theme.dart';

class MdControlledView extends StatefulWidget {
  final Widget? systemCards;
  const MdControlledView({super.key, this.systemCards});

  @override
  State<MdControlledView> createState() => _MdControlledViewState();
}

enum _HelpState { idle, sending, sent, failed }

class _MdControlledViewState extends State<MdControlledView> {
  final _issue = TextEditingController();
  var _help = _HelpState.idle;
  var _copied = false;

  @override
  void dispose() {
    _issue.dispose();
    super.dispose();
  }

  Future<void> _requestHelp() async {
    setState(() => _help = _HelpState.sending);
    try {
      final api = await bind.mainGetApiServer();
      final body = jsonEncode({
        'id': await bind.mainGetMyId(),
        'uuid': await bind.mainGetUuid(),
        'message': _issue.text.trim(),
      });
      final resp = await http.post(Uri.parse('$api/api/mydesk/help'),
          headers: {'Content-Type': 'application/json'}, body: body);
      setState(() => _help =
          resp.statusCode == 200 ? _HelpState.sent : _HelpState.failed);
    } catch (e) {
      debugPrint('request help failed: $e');
      setState(() => _help = _HelpState.failed);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = MdColors.of(context);
    final org = bind.mainGetBuildinOption(key: 'mydesk-org-name');
    return ChangeNotifierProvider.value(
      value: gFFI.serverModel,
      child: Container(
        color: c.bg,
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 460),
            child: ListView(
              padding: const EdgeInsets.fromLTRB(24, 20, 24, 16),
              children: [
                Row(children: [
                  const MdLogo(),
                  const SizedBox(width: 12),
                  Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text('MyDesk', style: mdTitle(c)),
                    Text(org.isEmpty ? 'IT 远程支持' : '$org IT 远程支持',
                        style: mdBody(c.text3, size: 12)),
                  ]),
                ]),
                if (widget.systemCards != null) widget.systemCards!,
                const SizedBox(height: 16),
                _idCard(c),
                const SizedBox(height: 16),
                Consumer<ServerModel>(builder: (_, m, __) => _sessionCard(c, m)),
                const SizedBox(height: 16),
                _helpArea(c),
                const SizedBox(height: 12),
                const OnlineStatusWidget(),
                const SizedBox(height: 8),
                Center(
                  child: TextButton(
                    onPressed: () => loginDialog(),
                    child: Text('运维人员登录 ›',
                        style: mdBody(c.accent, size: 13)),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _idCard(MdColors c) {
    final model = gFFI.serverModel;
    return Container(
      padding: const EdgeInsets.all(20),
      decoration: BoxDecoration(
          color: c.surface,
          border: Border.all(color: c.line),
          borderRadius: BorderRadius.circular(12)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text('本机 ID（告诉 IT 人员此号码）', style: mdBody(c.text3, size: 13)),
        const SizedBox(height: 8),
        Row(children: [
          Expanded(
            child: ValueListenableBuilder<TextEditingValue>(
              valueListenable: model.serverId,
              builder: (_, v, __) => SelectableText(
                v.text.isEmpty ? '…' : v.text,
                style: mdBody(c.text, size: 30, weight: FontWeight.w700)
                    .copyWith(letterSpacing: 2),
              ),
            ),
          ),
          Tooltip(
            message: '复制本机 ID',
            child: MdButton(_copied ? '已复制' : '复制', outlined: true,
                onPressed: () {
              Clipboard.setData(ClipboardData(text: model.serverId.text));
              setState(() => _copied = true);
            }),
          ),
        ]),
      ]),
    );
  }

  Widget _sessionCard(MdColors c, ServerModel m) {
    final active = m.clients.where((x) => x.authorized && !x.disconnected).toList();
    if (active.isEmpty) {
      return _card(c.inset, null, [
        Row(children: [
          Icon(Icons.verified_user_outlined, color: c.text3, size: 22),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('当前没有人连接这台电脑',
                  style: mdBody(c.text, weight: FontWeight.w500)),
              Text('有人连接时这里和系统托盘都会提示',
                  style: mdBody(c.text3, size: 12)),
            ]),
          ),
        ]),
      ]);
    }
    return _card(c.warnBg, c.warn, [
      for (final cl in active) ...[
        Row(children: [
          Icon(Icons.screen_share_outlined, color: c.warn, size: 22),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('${cl.name.isEmpty ? cl.peerId : cl.name} 正在远程协助您',
                  style: mdBody(c.warn, weight: FontWeight.w700)),
              Text('对方可以看到并操作您的屏幕',
                  style: mdBody(c.text2, size: 12)),
            ]),
          ),
        ]),
        const SizedBox(height: 12),
        SizedBox(
          width: double.infinity,
          child: MdButton('立即断开', danger: true, height: 44,
              onPressed: () => bind.cmCloseConnection(connId: cl.id)),
        ),
      ],
    ]);
  }

  Widget _helpArea(MdColors c) {
    switch (_help) {
      case _HelpState.idle:
      case _HelpState.sending:
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          TextField(
            controller: _issue,
            maxLength: 200,
            decoration: InputDecoration(
              labelText: '问题描述（可选）',
              hintText: '例如：打印机连不上',
              border: OutlineInputBorder(borderRadius: BorderRadius.circular(8)),
              counterText: '',
            ),
          ),
          const SizedBox(height: 10),
          MdButton(_help == _HelpState.sending ? '正在通知…' : '请求 IT 协助',
              height: 48,
              onPressed: _help == _HelpState.sending ? null : _requestHelp),
        ]);
      case _HelpState.sent:
        return _card(c.infoBg, null, [
          Text('已通知 IT 人员，请保持电脑开机',
              style: mdBody(c.info, weight: FontWeight.w700)),
          const SizedBox(height: 8),
          Align(
            alignment: Alignment.centerLeft,
            child: MdButton('好的', outlined: true, height: 36,
                onPressed: () => setState(() {
                      _help = _HelpState.idle;
                      _issue.clear();
                    })),
          ),
        ]);
      case _HelpState.failed:
        return _card(c.dangerBg, null, [
          Text('通知失败，请检查网络或直接电话联系 IT',
              style: mdBody(c.danger, weight: FontWeight.w700)),
          const SizedBox(height: 8),
          Align(
            alignment: Alignment.centerLeft,
            child: MdButton('重试', outlined: true, height: 36,
                onPressed: _requestHelp),
          ),
        ]);
    }
  }

  Widget _card(Color bg, Color? border, List<Widget> children) => Container(
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
            color: bg,
            border: border == null ? null : Border.all(color: border),
            borderRadius: BorderRadius.circular(12)),
        child: Column(
            crossAxisAlignment: CrossAxisAlignment.start, children: children),
      );
}
