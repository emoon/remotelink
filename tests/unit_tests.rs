use remotelink::messages;

#[test]
fn test_message_from_u8_valid() {
    // Test valid message types (0-20 inclusive)
    for i in 0..=20 {
        assert!(
            messages::Messages::from_u8(i).is_ok(),
            "Message {} should be valid",
            i
        );
    }
}

#[test]
fn test_message_from_u8_invalid() {
    // Test invalid message types (21 and above)
    assert!(messages::Messages::from_u8(21).is_err());
    assert!(messages::Messages::from_u8(100).is_err());
    assert!(messages::Messages::from_u8(255).is_err());
}

#[test]
fn test_message_from_u8_boundary() {
    // Test boundary conditions (highest valid message is 20)
    let valid_max = 20;
    assert!(messages::Messages::from_u8(valid_max).is_ok());
    assert!(messages::Messages::from_u8(valid_max + 1).is_err());
}

#[test]
fn test_message_types_match_enum() {
    // Verify that message types match expected enum values
    assert_eq!(
        messages::Messages::from_u8(0).unwrap() as u8,
        messages::Messages::HandshakeRequest as u8
    );
    assert_eq!(
        messages::Messages::from_u8(1).unwrap() as u8,
        messages::Messages::HandshakeReply as u8
    );
    assert_eq!(
        messages::Messages::from_u8(6).unwrap() as u8,
        messages::Messages::StdoutOutput as u8
    );
    assert_eq!(
        messages::Messages::from_u8(7).unwrap() as u8,
        messages::Messages::StderrOutput as u8
    );
}
