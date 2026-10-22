//! The end-to-end happy path the MVP is defined by: two users authenticate,
//! one lights a room, the other joins, and a relay reaches the member.

use std::sync::Arc;

use qr_race_core::{Capital, Compression, PayloadKind, RoomAddress, RoomRole, UserId};
use qr_race_hub::auth::AuthService;
use qr_race_hub::clock::{Clock, ManualClock};
use qr_race_hub::hub::{Hub, RelayRequest};
use qr_race_hub::{AuthError, generate_token, generate_totp_secret_base32, totp};

#[test]
fn auth_then_room_then_relay() {
    let clock = ManualClock::new(1_700_000_000_000);
    let clock_arc: Arc<dyn Clock> = Arc::new(clock.clone());

    let mut auth = AuthService::new(clock_arc.clone());
    let host_token = generate_token();
    let host_secret = generate_totp_secret_base32();
    let member_token = generate_token();
    let member_secret = generate_totp_secret_base32();

    let host_id = UserId("host-a".into());
    let member_id = UserId("m-1".into());
    auth.provision(host_id.clone(), &host_token, &host_secret)
        .unwrap();
    auth.provision(member_id.clone(), &member_token, &member_secret)
        .unwrap();

    let host_session = auth
        .authenticate(
            &host_token,
            &totp::current_code(&host_secret, clock.now_ms(), 6).unwrap(),
        )
        .unwrap();
    let member_session = auth
        .authenticate(
            &member_token,
            &totp::current_code(&member_secret, clock.now_ms(), 6).unwrap(),
        )
        .unwrap();
    assert_ne!(host_session.id, member_session.id);
    assert_eq!(host_session.user_id, host_id);

    let mut hub = Hub::new(clock_arc);
    let address = RoomAddress::new(Capital::Tokyo, "A", "1B", "103").unwrap();

    hub.create_room(address.clone(), host_session.user_id.clone())
        .unwrap();
    hub.light_room(&host_session.user_id, &address).unwrap();
    hub.join(member_session.user_id.clone(), &address).unwrap();

    let events = hub
        .publish_relay(
            &host_session.user_id,
            &address,
            RelayRequest {
                kind: PayloadKind::Text,
                compression: Compression::None,
                payload: b"first relay".to_vec(),
            },
        )
        .unwrap();
    assert_eq!(events.len(), 1);

    let view = hub.view(&member_session.user_id, &address).unwrap();
    assert_eq!(view.relays.len(), 1);
    assert_eq!(&*view.relays[0].payload, b"first relay");
    assert_eq!(view.relays[0].publisher, host_id);

    let member_view = view
        .members
        .iter()
        .find(|member| member.user == member_id)
        .unwrap();
    assert_eq!(member_view.role, RoomRole::Member);

    // The step that authenticated the host cannot be used a second time.
    let replay = totp::current_code(&host_secret, clock.now_ms(), 6).unwrap();
    assert_eq!(
        auth.authenticate(&host_token, &replay),
        Err(AuthError::ReplayedTotp)
    );
}

#[test]
fn a_member_cannot_publish_even_after_joining() {
    let clock = ManualClock::new(1_700_000_000_000);
    let clock_arc: Arc<dyn Clock> = Arc::new(clock.clone());
    let mut hub = Hub::new(clock_arc);

    let address = RoomAddress::new(Capital::Kyoto, "Z", "B", "103").unwrap();
    let host = UserId("host-b".into());
    let member = UserId("m-2".into());

    hub.create_room(address.clone(), host.clone()).unwrap();
    hub.light_room(&host, &address).unwrap();
    hub.join(member.clone(), &address).unwrap();

    assert_eq!(
        hub.publish_relay(
            &member,
            &address,
            RelayRequest {
                kind: PayloadKind::Url,
                compression: Compression::None,
                payload: b"https://example.test/x".to_vec(),
            },
        ),
        Err(qr_race_hub::HubError::NotAScanner)
    );
}
