import 'package:flutter_test/flutter_test.dart';
import 'package:qr_race_gui/api/connect_link.dart';

const _fingerprint =
    '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';

void main() {
  test('self-signed HTTPS link carries the pinned fingerprint', () {
    final link = buildConnectLink(
      server: 'https://127.0.0.1:8443',
      fingerprint: _fingerprint,
      token: 'secret-token',
      room: '东京A区1B级103房',
    );
    expect(link, startsWith('qrrace://connect?'));
    expect(link, contains('fingerprint=$_fingerprint'));
    final parsed = parseConnectLink(link)!;
    expect(parsed.server, 'https://127.0.0.1:8443');
    expect(parsed.fingerprint, _fingerprint);
    expect(parsed.token, 'secret-token');
    expect(parsed.room, '东京A区1B级103房');
  });

  test('refuses a self-signed link without a fingerprint', () {
    expect(
      () => buildConnectLink(
        server: 'https://192.168.0.3:8443',
        token: 't',
      ),
      throwsArgumentError,
    );
    expect(
      () => buildConnectLink(
        server: 'https://localhost:8443',
        fingerprint: 'abc123',
      ),
      throwsArgumentError,
    );
  });

  test('public HTTPS may omit the pinned fingerprint', () {
    final link = buildConnectLink(
      server: 'https://race.example.com',
      token: 't',
    );
    final parsed = parseConnectLink(link)!;
    expect(parsed.fingerprint, isNull);
    expect(parsed.server, 'https://race.example.com');
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
