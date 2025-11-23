use remotelink::messages;

#[test]
fn test_message_from_u8_valid() {
    // Test valid message types
    assert!(messages::Messages::from_u8(0).is_ok());
    assert!(messages::Messages::from_u8(1).is_ok());
    assert!(messages::Messages::from_u8(2).is_ok());
    assert!(messages::Messages::from_u8(3).is_ok());
    assert!(messages::Messages::from_u8(4).is_ok());
    assert!(messages::Messages::from_u8(5).is_ok());
    assert!(messages::Messages::from_u8(6).is_ok());
    assert!(messages::Messages::from_u8(7).is_ok());
    assert!(messages::Messages::from_u8(8).is_ok());
}

#[test]
fn test_message_from_u8_invalid() {
    // Test invalid message types
    assert!(messages::Messages::from_u8(9).is_err());
    assert!(messages::Messages::from_u8(10).is_err());
    assert!(messages::Messages::from_u8(255).is_err());
}

#[test]
fn test_message_from_u8_boundary() {
    // Test boundary conditions
    let valid_max = 8;
    assert!(messages::Messages::from_u8(valid_max).is_ok());
    assert!(messages::Messages::from_u8(valid_max + 1).is_err());
}

#[test]
fn test_message_types_match_enum() {
    // Verify that message types match expected enum values
    assert_eq!(messages::Messages::from_u8(0).unwrap() as u8, messages::Messages::HandshakeRequest as u8);
    assert_eq!(messages::Messages::from_u8(1).unwrap() as u8, messages::Messages::HandshakeReply as u8);
    assert_eq!(messages::Messages::from_u8(6).unwrap() as u8, messages::Messages::StdoutOutput as u8);
    assert_eq!(messages::Messages::from_u8(7).unwrap() as u8, messages::Messages::StderrOutput as u8);
}
