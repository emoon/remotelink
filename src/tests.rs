#[cfg(test)]
mod remote_link {
    #[test]
    fn fistbump_ok() {
        let mut stream = Vec::new();
        let mut out_stream = Vec::with_capacity(1024);

        let fistbump_request = FistbumpRequest {
            msg_type: Messages::FistbumpRequest as u8,
            version_major: 2,
            version_minor: 3,
        };

        send_message(stream, &fistbump_request).unwrap();
        get_message(stream, &mut out_stream).unwrap();
    }
}
