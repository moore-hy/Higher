//! LAN Transport（DEV-SYNC-001 §十五）。
//!
//! 简单 TCP + length-prefixed JSON：4 byte 大端长度 + UTF-8 JSON。
//! 单包上限 10 MB，超限直接拒绝。仅使用 std::net，无第三方网络栈。

use std::io::{Read, Write};
use std::net::TcpStream;

use super::types::WireMessage;

pub const MAX_PACKET: usize = 10 * 1024 * 1024;

#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    Json(serde_json::Error),
    PacketTooLarge(usize),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "网络错误：{e}"),
            Self::Json(e) => write!(f, "协议解析错误：{e}"),
            Self::PacketTooLarge(n) => write!(f, "数据包超限（{n} bytes > 10 MB），已拒绝"),
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for TransportError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub fn write_message(stream: &mut TcpStream, msg: &WireMessage) -> Result<(), TransportError> {
    let bytes = serde_json::to_vec(msg)?;
    if bytes.len() > MAX_PACKET {
        return Err(TransportError::PacketTooLarge(bytes.len()));
    }
    let len = u32::try_from(bytes.len()).map_err(|_| TransportError::PacketTooLarge(bytes.len()))?;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

pub fn read_message(stream: &mut TcpStream) -> Result<WireMessage, TransportError> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_PACKET {
        return Err(TransportError::PacketTooLarge(len));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}
