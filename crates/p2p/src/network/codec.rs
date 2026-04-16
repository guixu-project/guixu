// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use libp2p::futures::prelude::*;
use libp2p::request_response;
use std::io;

/// Simple JSON-over-length-prefix codec for request-response protocols.
/// Both request and response are `Vec<u8>` (JSON-serialized by callers).
#[derive(Debug, Clone, Default)]
pub struct JsonCodec;

#[async_trait::async_trait]
impl request_response::Codec for JsonCodec {
    type Protocol = libp2p::StreamProtocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_length_prefixed(io, 1024 * 1024).await
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_length_prefixed(io, 1024 * 1024).await
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, &req).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        resp: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, &resp).await
    }
}

async fn read_length_prefixed<T>(io: &mut T, max_len: usize) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin + Send,
{
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {len} > {max_len}"),
        ));
    }
    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_length_prefixed<T>(io: &mut T, data: &[u8]) -> io::Result<()>
where
    T: AsyncWrite + Unpin + Send,
{
    let len = (data.len() as u32).to_be_bytes();
    io.write_all(&len).await?;
    io.write_all(data).await?;
    io.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::futures::io::AllowStdIo;
    use libp2p::request_response::Codec as _;

    #[tokio::test]
    async fn roundtrip_request() {
        let protocol = libp2p::StreamProtocol::new("/test/1.0.0");
        let data = b"hello world request".to_vec();

        let mut buf = Vec::new();
        let mut codec = JsonCodec;
        codec
            .write_request(&protocol, &mut AllowStdIo::new(&mut buf), data.clone())
            .await
            .unwrap();

        let mut cursor = AllowStdIo::new(std::io::Cursor::new(buf));
        let decoded = codec.read_request(&protocol, &mut cursor).await.unwrap();
        assert_eq!(decoded, data);
    }

    #[tokio::test]
    async fn roundtrip_response() {
        let protocol = libp2p::StreamProtocol::new("/test/1.0.0");
        let data = b"{\"key\":\"value\"}".to_vec();

        let mut buf = Vec::new();
        let mut codec = JsonCodec;
        codec
            .write_response(&protocol, &mut AllowStdIo::new(&mut buf), data.clone())
            .await
            .unwrap();

        let mut cursor = AllowStdIo::new(std::io::Cursor::new(buf));
        let decoded = codec.read_response(&protocol, &mut cursor).await.unwrap();
        assert_eq!(decoded, data);
    }

    #[tokio::test]
    async fn rejects_oversized_message() {
        let protocol = libp2p::StreamProtocol::new("/test/1.0.0");
        // Write a length prefix claiming 2MB (over 1MB limit)
        let len = 2_000_000u32.to_be_bytes();
        let mut buf = Vec::new();
        buf.extend_from_slice(&len);
        buf.extend_from_slice(&[0u8; 100]);

        let mut cursor = AllowStdIo::new(std::io::Cursor::new(buf));
        let mut codec = JsonCodec;
        let result = codec.read_request(&protocol, &mut cursor).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn empty_payload_roundtrip() {
        let protocol = libp2p::StreamProtocol::new("/test/1.0.0");
        let data = Vec::new();

        let mut buf = Vec::new();
        let mut codec = JsonCodec;
        codec
            .write_request(&protocol, &mut AllowStdIo::new(&mut buf), data.clone())
            .await
            .unwrap();

        let mut cursor = AllowStdIo::new(std::io::Cursor::new(buf));
        let decoded = codec.read_request(&protocol, &mut cursor).await.unwrap();
        assert_eq!(decoded, data);
    }
}
