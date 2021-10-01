use crate::messages::Messages;
use crate::trace;
use anyhow::*;
use serde::ser::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::io::{Read, Write};

/// These are all the states that is needed to write to the output
/// This supports writing it non-blocking fashion and can pickup where it left of.
#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    ReadHeader,
    ReadData,
    WriteHeader,
    WriteData,
    /// Read or write has completed
    Complete,
}

/// This makes it possible to auto trasition the state matchine to read after
/// a write has been completed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TransitionToRead {
    Yes,
    No,
}

pub struct MessageStream {
    /// Set to Yes if transition to read after a write has been completed
    transition_to_read: TransitionToRead,
    /// Current state of the state machine
    state: State,
    /// header read or write offset (number of bytes read or written)
    message: Messages,
    /// header read or write offset (number of bytes read or written)
    header_offset: usize,
    /// how much data that has been read to the data buffer
    data_offset: usize,
    /// header to read/write from
    header: [u8; 8],
    /// Data to read/write from
    pub data: Vec<u8>,
}

impl MessageStream {
    pub fn new() -> MessageStream {
        MessageStream {
            transition_to_read: TransitionToRead::No,
            state: State::Complete,
            message: Messages::NoMessage,
            header_offset: 0,
            data_offset: 0,
            header: [0; 8],
            data: Vec::new(),
        }
    }

    /// Update the state machine. Will return a Some(Message) when a read request has finished.
    /// For writes no state will be given back
    pub fn update<S: Write + Read>(&mut self, stream: &mut S) -> Result<Option<Messages>> {
        if self.state != State::Complete {
            trace!("update state {:?}", self.state);
        }

        match self.state {
            State::WriteHeader => {
                self.write_header(stream)?;
                // We have switched state to write data, we try to write it here as well
                // to finish it as early as possible
                if self.state == State::WriteData {
                    dbg!();
                    self.write_data(stream)?;
                }

                Ok(None)
            }

            State::WriteData => {
                self.write_data(stream)?;
                Ok(None)
            }

            State::ReadHeader => {
                self.read_header(stream)?;
                // Read data directly here if we are finished with the header
                if self.state == State::ReadData {
                    self.data_offset = 0;
                    self.read_data(stream)
                } else {
                    Ok(None)
                }
            }

            State::ReadData => {
                dbg!();
                self.read_data(stream)
            }

            State::Complete => Ok(None),
        }
    }

    /// Will return false if read can't be started (write/read in progress)
    pub fn begin_read<S: Write + Read>(
        &mut self,
        stream: &mut S,
        do_update: bool,
    ) -> Result<Option<Messages>> {
        if self.state != State::Complete && self.state != State::ReadHeader {
            trace!("begin_read: Not started, not correct state");
            Ok(None)
        } else {
            trace!("begin_read: Start reading header");
            self.header_offset = 0;
            self.data_offset = 0;
            self.state = State::ReadHeader;
            if do_update {
                self.update(stream)
            } else {
                Ok(None)
            }
        }
    }

    /// Begins writing message to the stream, returns false if it can't, true if finished
    pub fn begin_write_message<T: Serialize, S: Write + Read>(
        &mut self,
        stream: &mut S,
        data: &T,
        msg_type: Messages,
        trans_to_read: TransitionToRead,
    ) -> Result<bool> {
        // Make sure we can make progress
        if self.state != State::Complete {
            return Ok(false);
        }

        bincode::serialize_into(&mut self.data, data)?;

        let len = self.data.len() as u64;
        // reserve upper space for type
        assert!(len < 0xffff_ffff_ffff);
        // store type in top byte
        self.header[0] = msg_type as u8;
        self.header[1] = ((len >> 48) & 0xff) as u8;
        self.header[2] = ((len >> 40) & 0xff) as u8;
        self.header[3] = ((len >> 32) & 0xff) as u8;
        self.header[4] = ((len >> 24) & 0xff) as u8;
        self.header[5] = ((len >> 16) & 0xff) as u8;
        self.header[6] = ((len >> 8) & 0xff) as u8;
        self.header[7] = (len & 0xff) as u8;

        self.transition_to_read = trans_to_read;
        self.state = State::WriteHeader;
        self.header_offset = 0;

        trace!(
            "begin_write_message: {:?} len {}",
            msg_type,
            self.data.len()
        );

        // Do a update directly here to reduce latency as short messages will likely finish directly
        self.update(stream)?;

        // check if we have finished already
        if self.state == State::Complete || self.state == State::ReadHeader {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// handle writing to a socket that is non-blocking
    fn write<S: Write + Read>(stream: &mut S, data: &[u8]) -> Result<usize> {
        match stream.write(data) {
            Ok(n) => Ok(n),
            Err(err) => {
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    Ok(0)
                } else {
                    bail!(err);
                }
            }
        }
    }

    /// handle read to a socket that is non-blocking
    fn read<S: Write + Read>(data: &mut [u8], stream: &mut S) -> Result<usize> {
        match stream.read(data) {
            Ok(n) => Ok(n),
            Err(err) => {
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    Ok(0)
                } else {
                    bail!(err);
                }
            }
        }
    }

    /// Write header to the stream and return the total amount of data that has been written
    fn write_header<S: Write + Read>(&mut self, stream: &mut S) -> Result<usize> {
        self.header_offset += Self::write(stream, &self.header[self.header_offset..])?;
        trace!(
            "write_header: written total of {} bytes",
            self.header_offset
        );

        if self.header_offset == 8 {
            let mut hasher = DefaultHasher::new();
            hasher.write(&self.data);
            trace!(
                "write_data hash {:x} len {}",
                hasher.finish(),
                self.data.len()
            );

            self.data_offset = 0;
            self.state = State::WriteData;
        }

        Ok(self.header_offset)
    }

    /// Write data to the stream and return the total amount of data that has been written
    fn write_data<S: Write + Read>(&mut self, stream: &mut S) -> Result<usize> {
        self.data_offset += Self::write(stream, &self.data[self.data_offset..])?;
        trace!("write_data total bytes {} written", self.data_offset);

        // When we have finished writing all data we switch over to look for incoming messages
        if self.data_offset == self.data.len() {
            self.header_offset = 0;

            if self.transition_to_read == TransitionToRead::Yes {
                trace!("write_data done, tranistion to State::ReadHeader");
                self.state = State::ReadHeader;
            } else {
                trace!("write_data done, tranistion to State::Complete");
                self.state = State::Complete;
            }
        }

        Ok(self.data_offset)
    }

    /// Reads header data to self, returns number of bytes read
    fn read_header<S: Write + Read>(&mut self, stream: &mut S) -> Result<usize> {
        self.header_offset += Self::read(&mut self.header[self.header_offset..], stream)?;
        trace!("read_header total bytes {} read", self.header_offset);

        if self.header_offset == 8 {
            let msg_type = self.header[0];
            let size = (((self.header[1] as u64) << 48)
                | ((self.header[2] as u64) << 40)
                | ((self.header[3] as u64) << 32)
                | ((self.header[4] as u64) << 24)
                | ((self.header[5] as u64) << 16)
                | ((self.header[6] as u64) << 8)
                | (self.header[7] as u64)) as usize;

            assert!(size < 0xffff_ffff_ffff);
            // TODO: Optimize
            self.data.resize(size, 0xff);
            self.message = unsafe { std::mem::transmute(msg_type) };
            self.state = State::ReadData;
        }

        Ok(self.header_offset)
    }

    fn read_data<S: Write + Read>(&mut self, stream: &mut S) -> Result<Option<Messages>> {
        self.data_offset += Self::read(&mut self.data[self.data_offset..], stream)?;
        trace!("read_data total bytes {} read", self.data_offset);

        if self.data_offset == self.data.len() {
            let mut hasher = DefaultHasher::new();
            hasher.write(&self.data);
            trace!(
                "read_data hash {:x} len {}",
                hasher.finish(),
                self.data.len()
            );
            self.state = State::Complete;
            Ok(Some(self.message))
        } else {
            Ok(None)
        }
    }
}
