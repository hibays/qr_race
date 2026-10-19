import 'package:flutter/material.dart';

/// A venue console, not a social app: near-black ground, one signal green, and
/// white only where a QR symbol needs it.
const ground = Color(0xFF0A0E0B);
const surface = Color(0xFF121813);
const surfaceRaised = Color(0xFF19211A);
const line = Color(0xFF2A352C);
const ink = Color(0xFFE9F0EA);
const muted = Color(0xFF93A196);
const accent = Color(0xFF3DDC84);
const accentInk = Color(0xFF04220F);
const warn = Color(0xFFF0A44A);
const danger = Color(0xFFF26D6D);

ThemeData buildTheme() {
  final base = ThemeData(
    useMaterial3: true,
    brightness: Brightness.dark,
    scaffoldBackgroundColor: ground,
    colorScheme: const ColorScheme.dark(
      primary: accent,
      onPrimary: accentInk,
      secondary: accent,
      surface: surface,
      onSurface: ink,
      error: danger,
    ),
  );

  return base.copyWith(
    textTheme: base.textTheme.apply(bodyColor: ink, displayColor: ink),
    appBarTheme: const AppBarTheme(
      backgroundColor: ground,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      centerTitle: false,
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: surface,
      labelStyle: const TextStyle(color: muted),
      hintStyle: const TextStyle(color: muted),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: const BorderSide(color: line),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: const BorderSide(color: line),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: const BorderSide(color: accent, width: 1.4),
      ),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: accent,
        foregroundColor: accentInk,
        minimumSize: const Size.fromHeight(48),
        textStyle: const TextStyle(fontWeight: FontWeight.w700, letterSpacing: 0.2),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: ink,
        side: const BorderSide(color: line),
        minimumSize: const Size.fromHeight(48),
        textStyle: const TextStyle(fontWeight: FontWeight.w600),
      ),
    ),
    dividerColor: line,
    cardTheme: const CardThemeData(
      color: surfaceRaised,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      margin: EdgeInsets.zero,
    ),
    snackBarTheme: const SnackBarThemeData(
      backgroundColor: surfaceRaised,
      contentTextStyle: TextStyle(color: ink),
    ),
  );
}
