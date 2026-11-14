// MyDesk 设计令牌：鉴色（中国传统色）。页面与组件只用这里的语义色，不写死色值。
import 'package:flutter/material.dart';

class MdColors {
  final Color bg, surface, inset, line, lineStrong;
  final Color text, text2, text3;
  final Color accent, accentSoft, onAccent;
  final Color ok, warn, warnBg, danger, dangerBg, onDanger, info, infoBg, mute;

  const MdColors({
    required this.bg,
    required this.surface,
    required this.inset,
    required this.line,
    required this.lineStrong,
    required this.text,
    required this.text2,
    required this.text3,
    required this.accent,
    required this.accentSoft,
    required this.onAccent,
    required this.ok,
    required this.warn,
    required this.warnBg,
    required this.danger,
    required this.dangerBg,
    required this.onDanger,
    required this.info,
    required this.infoBg,
    required this.mute,
  });

  // 浅色：汉白玉 / 象牙白 / 米白；主色苍筤
  static const light = MdColors(
    bg: Color(0xFFF8F4ED),
    surface: Color(0xFFFFFEF8),
    inset: Color(0xFFF5F0E6),
    line: Color(0xFFDAD4CB),
    lineStrong: Color(0xFFBBB5AC),
    text: Color(0xFF1D1B1C),
    text2: Color(0xFF3B3A3E),
    text3: Color(0xFF424B50),
    accent: Color(0xFF2E5D4A),
    accentSoft: Color(0xFFEEF7F2),
    onAccent: Color(0xFFFFFEF8),
    ok: Color(0xFF2E5D4A),
    warn: Color(0xFF806332),
    warnBg: Color(0xFFF9F4DC),
    danger: Color(0xFF9E2F2F),
    dangerBg: Color(0xFFF2CAC9),
    onDanger: Color(0xFFFFFEF8),
    info: Color(0xFF2A3C5C),
    infoBg: Color(0xFFE6F2FF),
    mute: Color(0xFF867E76),
  );

  // 深色：玄色 / 墨韵黑 / 松烟墨；主色梅子青
  static const dark = MdColors(
    bg: Color(0xFF1A1A1A),
    surface: Color(0xFF2E2E2E),
    inset: Color(0xFF3B3A3E),
    line: Color(0xFF867E76),
    lineStrong: Color(0xFFA49C93),
    text: Color(0xFFF8F4ED),
    text2: Color(0xFFE4DFD7),
    text3: Color(0xFFBBB5AC),
    accent: Color(0xFF7BC092),
    accentSoft: Color(0xFF1A3B2E),
    onAccent: Color(0xFF1A1A1A),
    ok: Color(0xFF7BC092),
    warn: Color(0xFFF8C471),
    warnBg: Color(0xFF4D4030),
    danger: Color(0xFFF2B9B2),
    dangerBg: Color(0xFF481E1C),
    onDanger: Color(0xFF1A1A1A),
    info: Color(0xFFC3D7DF),
    infoBg: Color(0xFF1A3A5F),
    mute: Color(0xFFA49C93),
  );

  static MdColors of(BuildContext context) =>
      Theme.of(context).brightness == Brightness.dark ? dark : light;
}

/// 标题衬线、正文无衬线；字体缺失时回落系统字体。
const kMdSerif = ['Noto Serif SC', 'Songti SC', 'SimSun', 'serif'];
const kMdSans = ['Noto Sans SC', 'PingFang SC', 'Microsoft YaHei'];

TextStyle mdTitle(MdColors c, {double size = 18}) => TextStyle(
    fontFamilyFallback: kMdSerif,
    fontWeight: FontWeight.w700,
    fontSize: size,
    color: c.text);

TextStyle mdBody(Color color, {double size = 14, FontWeight? weight}) =>
    TextStyle(
        fontFamilyFallback: kMdSans,
        fontSize: size,
        color: color,
        fontWeight: weight);

/// 主按钮。
class MdButton extends StatelessWidget {
  final String label;
  final VoidCallback? onPressed;
  final bool danger;
  final bool outlined;
  final double height;
  const MdButton(this.label,
      {super.key,
      this.onPressed,
      this.danger = false,
      this.outlined = false,
      this.height = 40});

  @override
  Widget build(BuildContext context) {
    final c = MdColors.of(context);
    final bg = danger ? c.danger : c.accent;
    final fg = danger ? c.onDanger : c.onAccent;
    final shape = RoundedRectangleBorder(borderRadius: BorderRadius.circular(8));
    if (outlined) {
      return SizedBox(
        height: height,
        child: OutlinedButton(
          onPressed: onPressed,
          style: OutlinedButton.styleFrom(
              foregroundColor: c.text2,
              side: BorderSide(color: c.lineStrong),
              shape: shape),
          child: Text(label, style: mdBody(c.text2, size: 13)),
        ),
      );
    }
    return SizedBox(
      height: height,
      child: ElevatedButton(
        onPressed: onPressed,
        style: ElevatedButton.styleFrom(
            backgroundColor: bg,
            foregroundColor: fg,
            disabledBackgroundColor: c.inset,
            disabledForegroundColor: c.text3,
            elevation: 0,
            shape: shape),
        child: Text(label,
            style: mdBody(onPressed == null ? c.text3 : fg,
                size: 14, weight: FontWeight.w500)),
      ),
    );
  }
}

/// 品牌标：显示器图形。
class MdLogo extends StatelessWidget {
  final double size;
  const MdLogo({super.key, this.size = 36});
  @override
  Widget build(BuildContext context) {
    final c = MdColors.of(context);
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
          color: c.accent, borderRadius: BorderRadius.circular(size / 4)),
      child: Icon(Icons.desktop_windows_outlined,
          color: c.onAccent, size: size * 0.55),
    );
  }
}
