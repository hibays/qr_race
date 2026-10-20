import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/api/connect_link.dart';

void main() {
  test('builds and parses a connect link', () {
    final link = buildConnectLink(
      server: 'https://127.0.0.1:8443',
      fingerprint: 'abc123',
      token: 'secret-token',
    );
    expect(link, startsWith('qrrace://connect?'));
    final parsed = parseConnectLink(link)!;
    expect(parsed.server, 'https://127.0.0.1:8443');
    expect(parsed.fingerprint, 'abc123');
    expect(parsed.token, 'secret-token');
  });

  test('accepts a bare query and rejects unrelated text', () {
    final parsed = parseConnectLink(
      '?server=https%3A%2F%2Fexample.test%3A8443&token=t',
    )!;
    expect(parsed.server, 'https://example.test:8443');
    expect(parsed.token, 't');
    expect(parseConnectLink('hello world'), isNull);
    expect(parseConnectLink('qrrace://connect?token=x'), isNull);
  });
}
