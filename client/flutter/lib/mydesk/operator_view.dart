// MyDesk 运维端：按权限列出全部设备（来自服务端按分组生成的共享设备库），
// 一键连接（使用服务端托管的密码，无需输入），处理员工求助。
import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/models/ab_model.dart';
import 'package:flutter_hbb/models/peer_model.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:get/get.dart';

import '../utils/http_service.dart' as http;
import 'theme.dart';

const _kSharedPrefix = '设备库 · ';
const _kOnlineEvent = 'callback_query_onlines';
const _kHandlerName = 'mydesk-operator';

enum _View { all, online, help, group }

class MdTicket {
  final int id;
  final String deviceId, hostname, message;
  final int createdAt;
  MdTicket.fromJson(Map<String, dynamic> j)
      : id = j['id'] ?? 0,
        deviceId = j['device_id'] ?? '',
        hostname = j['hostname'] ?? '',
        message = j['message'] ?? '',
        createdAt = j['created_at'] ?? 0;
}

class MdOperatorView extends StatefulWidget {
  const MdOperatorView({super.key});
  @override
  State<MdOperatorView> createState() => _MdOperatorViewState();
}

class _MdOperatorViewState extends State<MdOperatorView> {
  final _search = TextEditingController();
  final _online = <String>{}.obs;
  final _tickets = <MdTicket>[].obs;
  var _view = _View.all;
  var _group = '';
  var _onlineOnly = false;
  String _toast = '';
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    platformFFI.registerEventHandler(_kOnlineEvent, _kHandlerName, (evt) async {
      final on = (evt['onlines'] as String? ?? '').split(',').where((s) => s.isNotEmpty);
      final off = (evt['offlines'] as String? ?? '').split(',').where((s) => s.isNotEmpty);
      _online.addAll(on);
      _online.removeAll(off);
    }, replace: true);
    _refresh(force: true);
    _timer = Timer.periodic(const Duration(seconds: 15), (_) => _refresh());
  }

  @override
  void dispose() {
    _timer?.cancel();
    platformFFI.unregisterEventHandler(_kOnlineEvent, _kHandlerName);
    _search.dispose();
    super.dispose();
  }

  Future<void> _refresh({bool force = false}) async {
    await gFFI.abModel.pullAb(
        force: force ? ForcePullAb.listAndCurrent : null, quiet: true);
    final ids = _allPeers().map((p) => p.id).toList();
    if (ids.isNotEmpty) bind.queryOnlines(ids: ids);
    await _loadTickets();
  }

  Future<void> _loadTickets() async {
    try {
      final api = await bind.mainGetApiServer();
      final resp = await http.get(Uri.parse('$api/api/mydesk/help'),
          headers: getHttpHeaders());
      if (resp.statusCode != 200) return;
      final list = jsonDecode(utf8.decode(resp.bodyBytes)) as List;
      _tickets.value = list.map((e) => MdTicket.fromJson(e)).toList();
    } catch (e) {
      debugPrint('load tickets failed: $e');
    }
  }

  Future<void> _closeTicket(MdTicket t) async {
    try {
      final api = await bind.mainGetApiServer();
      await http.post(Uri.parse('$api/api/mydesk/help/${t.id}/done'),
          headers: getHttpHeaders());
    } finally {
      await _loadTickets();
    }
  }

  /// 共享设备库：名称 → 设备。个人簿不在运维视图中展示。
  Map<String, List<Peer>> _groups() {
    final res = <String, List<Peer>>{};
    gFFI.abModel.addressbooks.forEach((name, ab) {
      if (ab.isPersonal()) return;
      final label = name.startsWith(_kSharedPrefix)
          ? name.substring(_kSharedPrefix.length)
          : name;
      res[label] = ab.peers.toList();
    });
    return res;
  }

  List<Peer> _allPeers() {
    final seen = <String>{};
    return _groups().values.expand((e) => e).where((p) => seen.add(p.id)).toList();
  }

  Peer? _peerById(String id) =>
      _allPeers().firstWhereOrNull((p) => p.id == id);

  List<Peer> _filtered() {
    final q = _search.text.trim().toLowerCase();
    Iterable<Peer> list = _view == _View.group
        ? (_groups()[_group] ?? const [])
        : _allPeers();
    if (_view == _View.online || _onlineOnly) {
      list = list.where((p) => _online.contains(p.id));
    }
    if (q.isNotEmpty) {
      list = list.where((p) =>
          '${p.id} ${p.alias} ${p.hostname} ${p.username} ${p.note}'
              .toLowerCase()
              .contains(q));
    }
    return list.toList();
  }

  void _connect(Peer p, {bool file = false, bool terminal = false}) {
    setState(() => _toast = '正在连接 ${_name(p)}（${p.id}）…');
    connect(context, p.id,
        password: p.password,
        isSharedPassword: p.password.isNotEmpty,
        isFileTransfer: file,
        isTerminal: terminal);
  }

  String _name(Peer p) => p.alias.isNotEmpty ? p.alias : (p.hostname.isNotEmpty ? p.hostname : p.id);

  @override
  Widget build(BuildContext context) {
    final c = MdColors.of(context);
    return Container(
      color: c.bg,
      child: Column(children: [
        _topBar(c),
        if (_toast.isNotEmpty) _toastBar(c),
        Expanded(
          child: Obx(() {
            // 依赖：地址簿、在线状态、工单任一变化即重绘
            gFFI.abModel.addressbooks.length;
            _online.length;
            _tickets.length;
            return Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              SizedBox(width: 220, child: _nav(c)),
              Expanded(child: _main(c)),
            ]);
          }),
        ),
      ]),
    );
  }

  Widget _topBar(MdColors c) {
    final user = gFFI.userModel;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 10),
      decoration: BoxDecoration(
          color: c.surface, border: Border(bottom: BorderSide(color: c.line))),
      child: Wrap(
        spacing: 16,
        runSpacing: 8,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          Row(mainAxisSize: MainAxisSize.min, children: [
            const MdLogo(size: 28),
            const SizedBox(width: 10),
            Text('MyDesk 运维台', style: mdTitle(c, size: 16)),
          ]),
          SizedBox(
            width: 380,
            height: 40,
            child: TextField(
              controller: _search,
              onChanged: (_) => setState(() {}),
              decoration: InputDecoration(
                prefixIcon: Icon(Icons.search, color: c.text3, size: 18),
                hintText: '搜索主机名、ID、使用人',
                filled: true,
                fillColor: c.inset,
                contentPadding: EdgeInsets.zero,
                border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(8),
                    borderSide: BorderSide(color: c.lineStrong)),
              ),
            ),
          ),
          FilterChip(
            label: Text('仅看在线', style: mdBody(_onlineOnly ? c.accent : c.text2, size: 13)),
            selected: _onlineOnly,
            selectedColor: c.accentSoft,
            onSelected: (v) => setState(() => _onlineOnly = v),
          ),
          IconButton(
            tooltip: '刷新',
            icon: Icon(Icons.refresh, color: c.text2),
            onPressed: () => _refresh(force: true),
          ),
          Obx(() => Row(mainAxisSize: MainAxisSize.min, children: [
                CircleAvatar(
                  radius: 16,
                  backgroundColor: c.accentSoft,
                  child: Text(
                      user.userName.value.isEmpty
                          ? '?'
                          : user.userName.value.substring(0, 1).toUpperCase(),
                      style: mdBody(c.accent, weight: FontWeight.w700)),
                ),
                const SizedBox(width: 8),
                Text(user.userName.value, style: mdBody(c.text, size: 13)),
                if (user.isAdmin.value)
                  Text(' · 管理员', style: mdBody(c.text3, size: 12)),
                TextButton(
                  onPressed: () => user.logOut(),
                  child: Text('退出', style: mdBody(c.accent, size: 13)),
                ),
              ])),
        ],
      ),
    );
  }

  Widget _toastBar(MdColors c) => Container(
        margin: const EdgeInsets.fromLTRB(20, 12, 20, 0),
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
        decoration: BoxDecoration(
            color: c.accentSoft, borderRadius: BorderRadius.circular(10)),
        child: Row(children: [
          Expanded(child: Text(_toast, style: mdBody(c.accent))),
          TextButton(
              onPressed: () => setState(() => _toast = ''),
              child: Text('知道了', style: mdBody(c.accent, size: 12))),
        ]),
      );

  Widget _navItem(MdColors c, String label, int count, bool selected, VoidCallback onTap,
      {bool badge = false}) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Material(
        color: selected ? c.accentSoft : Colors.transparent,
        borderRadius: BorderRadius.circular(8),
        child: InkWell(
          borderRadius: BorderRadius.circular(8),
          onTap: onTap,
          child: Container(
            height: 40,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Row(children: [
              Expanded(
                child: Text(label,
                    overflow: TextOverflow.ellipsis,
                    style: mdBody(selected ? c.accent : c.text2,
                        weight: selected ? FontWeight.w700 : FontWeight.w400)),
              ),
              if (badge && count > 0)
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
                  decoration: BoxDecoration(
                      color: c.danger, borderRadius: BorderRadius.circular(11)),
                  child: Text('$count', style: mdBody(c.onDanger, size: 12)),
                )
              else
                Text('$count', style: mdBody(selected ? c.accent : c.text3, size: 12)),
            ]),
          ),
        ),
      ),
    );
  }

  Widget _nav(MdColors c) {
    final groups = _groups();
    final all = _allPeers();
    void pick(_View v, [String g = '']) => setState(() {
          _view = v;
          _group = g;
        });
    return ListView(padding: const EdgeInsets.all(16), children: [
      _navItem(c, '全部设备', all.length, _view == _View.all, () => pick(_View.all)),
      _navItem(c, '在线', all.where((p) => _online.contains(p.id)).length,
          _view == _View.online, () => pick(_View.online)),
      Padding(
        padding: const EdgeInsets.fromLTRB(12, 12, 12, 4),
        child: Text('分组', style: mdBody(c.text3, size: 12)),
      ),
      for (final e in groups.entries)
        _navItem(c, e.key, e.value.length,
            _view == _View.group && _group == e.key, () => pick(_View.group, e.key)),
      Padding(
        padding: const EdgeInsets.fromLTRB(12, 12, 12, 4),
        child: Text('工单', style: mdBody(c.text3, size: 12)),
      ),
      _navItem(c, '求助请求', _tickets.length, _view == _View.help,
          () => pick(_View.help),
          badge: true),
    ]);
  }

  Widget _main(MdColors c) {
    if (_view == _View.help) return _helpList(c);
    final list = _filtered();
    final title = switch (_view) {
      _View.all => '全部设备',
      _View.online => '在线设备',
      _View.group => _group,
      _View.help => '求助请求',
    };
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 16, 20, 16),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          Text(title, style: mdTitle(c, size: 22)),
          const SizedBox(width: 12),
          Text('${list.length} 台', style: mdBody(c.text3, size: 13)),
        ]),
        const SizedBox(height: 12),
        Expanded(
          child: Container(
            decoration: BoxDecoration(
                color: c.surface,
                border: Border.all(color: c.line),
                borderRadius: BorderRadius.circular(12)),
            child: list.isEmpty
                ? Center(
                    child: Text(
                        gFFI.abModel.currentAbLoading.value
                            ? '正在加载设备…'
                            : '没有符合条件的设备。试试清空搜索或关闭“仅看在线”。',
                        style: mdBody(c.text3)))
                : ListView.separated(
                    itemCount: list.length,
                    separatorBuilder: (_, __) => Divider(height: 1, color: c.line),
                    itemBuilder: (_, i) => _row(c, list[i]),
                  ),
          ),
        ),
        const SizedBox(height: 8),
        Text('点击“远程控制”直接连接，无需输入密码：服务端按你的权限下发托管密码，并记录审计日志。',
            style: mdBody(c.text3, size: 12)),
      ]),
    );
  }

  Widget _row(MdColors c, Peer p) {
    final online = _online.contains(p.id);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      child: Row(children: [
        Icon(Icons.circle, size: 9, color: online ? c.ok : c.mute),
        const SizedBox(width: 12),
        Expanded(
          flex: 3,
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(_name(p), overflow: TextOverflow.ellipsis,
                style: mdBody(c.text, weight: FontWeight.w500)),
            Text(p.note.isNotEmpty ? p.note : p.username,
                overflow: TextOverflow.ellipsis, style: mdBody(c.text3, size: 12)),
          ]),
        ),
        Expanded(flex: 2, child: Text(p.id, style: mdBody(c.text2))),
        Expanded(flex: 2, child: Text(p.platform, style: mdBody(c.text2))),
        Expanded(
          flex: 2,
          child: Text(online ? '在线' : '离线',
              style: mdBody(online ? c.ok : c.text3, size: 13)),
        ),
        MdButton('远程控制', height: 36, onPressed: online ? () => _connect(p) : null),
        const SizedBox(width: 6),
        PopupMenuButton<String>(
          tooltip: '文件传输与终端',
          enabled: online,
          icon: Icon(Icons.more_horiz, color: c.text2),
          onSelected: (v) => _connect(p, file: v == 'file', terminal: v == 'terminal'),
          itemBuilder: (_) => const [
            PopupMenuItem(value: 'file', child: Text('文件传输')),
            PopupMenuItem(value: 'terminal', child: Text('终端')),
          ],
        ),
      ]),
    );
  }

  Widget _helpList(MdColors c) {
    final list = _tickets.toList();
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 16, 20, 16),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text('求助请求', style: mdTitle(c, size: 22)),
        const SizedBox(height: 12),
        if (list.isEmpty)
          Text('暂无待处理的求助。', style: mdBody(c.text3))
        else
          Expanded(
            child: GridView.extent(
              maxCrossAxisExtent: 360,
              mainAxisSpacing: 12,
              crossAxisSpacing: 12,
              childAspectRatio: 2.2,
              children: [
                for (final t in list)
                  Container(
                    padding: const EdgeInsets.all(16),
                    decoration: BoxDecoration(
                        color: c.surface,
                        border: Border.all(color: c.line),
                        borderRadius: BorderRadius.circular(12)),
                    child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text('${t.hostname.isEmpty ? t.deviceId : t.hostname} · ${t.deviceId}',
                              style: mdBody(c.text3, size: 12)),
                          const SizedBox(height: 6),
                          Expanded(
                            child: Text(t.message.isEmpty ? '（未填写问题描述）' : '“${t.message}”',
                                style: mdBody(c.text, weight: FontWeight.w500)),
                          ),
                          Row(children: [
                            MdButton('立即协助', height: 36, onPressed: () {
                              final p = _peerById(t.deviceId);
                              if (p != null) {
                                _connect(p);
                              } else {
                                connect(context, t.deviceId);
                              }
                            }),
                            const SizedBox(width: 8),
                            MdButton('标记已处理', outlined: true, height: 36,
                                onPressed: () => _closeTicket(t)),
                          ]),
                        ]),
                  ),
              ],
            ),
          ),
      ]),
    );
  }
}
