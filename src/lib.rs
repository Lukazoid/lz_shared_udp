extern crate futures;
#[macro_use]
extern crate log;
#[macro_use]
extern crate tokio_core;

use std::ops::Deref;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use futures::{Async, AsyncSink, Poll, Sink, StartSend, Stream};
use tokio_core::net::{UdpCodec, UdpSocket};

#[must_use = "sinks do nothing unless polled"]
#[derive(Debug, Clone)]
pub struct SharedUdpFramed<R, C> {
    socket: R,
    codec: C,
    out_addr: SocketAddr,
    rd: Vec<u8>,
    wr: Vec<u8>,
    flushed: bool,
}

impl<R, C> SharedUdpFramed<R, C> {
    pub(crate) fn new(socket: R, codec: C) -> Self {
        Self {
            socket: socket,
            codec: codec,
            out_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
            rd: vec![0; 64 * 1024],
            wr: Vec::with_capacity(8 * 1024),
            flushed: true,
        }
    }
}

impl<R: Deref<Target = UdpSocket>, C: UdpCodec> Stream for SharedUdpFramed<R, C> {
    type Item = C::In;
    type Error = IoError;

    fn poll(&mut self) -> Poll<Option<C::In>, IoError> {
        let (n, addr) = try_nb!(self.socket.recv_from(&mut self.rd));
        trace!("received {} bytes, decoding", n);
        let frame = try!(self.codec.decode(&addr, &self.rd[..n]));
        trace!("frame decoded from buffer");
        Ok(Async::Ready(Some(frame)))
    }
}

impl<R: Deref<Target = UdpSocket>, C: UdpCodec> Sink for SharedUdpFramed<R, C> {
    type SinkItem = C::Out;
    type SinkError = IoError;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        trace!("sending frame");

        if !self.flushed {
            match self.poll_complete()? {
                Async::Ready(()) => {}
                Async::NotReady => return Ok(AsyncSink::NotReady(item)),
            }
        }

        self.out_addr = self.codec.encode(item, &mut self.wr);
        self.flushed = false;
        trace!("frame encoded; length={}", self.wr.len());

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), IoError> {
        if self.flushed {
            return Ok(Async::Ready(()));
        }

        trace!("flushing frame; length={}", self.wr.len());
        let n = try_nb!(self.socket.send_to(&self.wr, &self.out_addr));
        trace!("written {}", n);

        let wrote_all = n == self.wr.len();
        self.wr.clear();
        self.flushed = true;

        if wrote_all {
            Ok(Async::Ready(()))
        } else {
            Err(IoError::new(
                IoErrorKind::Other,
                "failed to write entire datagram to socket",
            ))
        }
    }
}

pub trait SharedUdpSocket {
    fn framed<C: UdpCodec>(&self, codec: C) -> SharedUdpFramed<Self, C>
    where
        Self: Sized;
}

impl<R> SharedUdpSocket for R
where
    R: Clone + Deref<Target = UdpSocket>,
{
    fn framed<C: UdpCodec>(&self, codec: C) -> SharedUdpFramed<Self, C>
    where
        Self: Sized,
    {
        SharedUdpFramed::new(self.clone(), codec)
    }
}

#[cfg(test)]
mod tests {
    use super::SharedUdpSocket;
    use tokio_core::net::{UdpCodec, UdpSocket};
    use tokio_core::reactor::{Core, Handle};
    use futures::{Future, Sink, Stream};
    use std::io::Result as IoResult;
    use std::net::SocketAddr;
    use std::ops::Deref;
    use std::rc::Rc;
    use std::sync::Arc;

    fn bind_sockets(handle: &Handle) -> (UdpSocket, UdpSocket) {
        let any_address = "0.0.0.0:0".parse().unwrap();

        let first = UdpSocket::bind(&any_address, handle).unwrap();
        let second = UdpSocket::bind(&any_address, handle).unwrap();

        (first, second)
    }

    struct Utf8Codec;

    impl UdpCodec for Utf8Codec {
        type In = String;
        type Out = (SocketAddr, String);

        fn decode(&mut self, _: &SocketAddr, buf: &[u8]) -> IoResult<Self::In> {
            Ok(String::from_utf8_lossy(buf).into_owned())
        }

        fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> SocketAddr {
            buf.extend_from_slice(msg.1.as_bytes());

            msg.0
        }
    }

    #[test]
    fn works_for_ref_udp_socket() {
        let core = Core::new().unwrap();

        let (first_socket, second_socket) = bind_sockets(&core.handle());

        test_framed_impl(core, &first_socket, &second_socket);
    }

    #[test]
    fn works_for_rc_udp_socket() {
        let core = Core::new().unwrap();

        let (first_socket, second_socket) = bind_sockets(&core.handle());

        test_framed_impl(core, Rc::new(first_socket), Rc::new(second_socket));
    }

    #[test]
    fn works_for_arc_udp_socket() {
        let core = Core::new().unwrap();

        let (first_socket, second_socket) = bind_sockets(&core.handle());

        test_framed_impl(core, Arc::new(first_socket), Arc::new(second_socket));
    }

    fn test_framed_impl<R>(mut core: Core, first_socket: R, second_socket: R)
    where
        R: Clone + Deref<Target = UdpSocket>,
    {
        let loopback = "127.0.0.1".parse().unwrap();

        let mut second_socket_addr = second_socket.local_addr().unwrap();
        second_socket_addr.set_ip(loopback);

        let second_socket_stream = second_socket.framed(Utf8Codec);

        let sent_message = "Hello";
        let future = first_socket
            .framed(Utf8Codec)
            .send((second_socket_addr, sent_message.to_owned()))
            .and_then(move |_| {
                second_socket_stream
                    .into_future()
                    .map(|(msg, _)| msg.unwrap())
                    .map_err(|(err, _)| err)
            });

        let received_message = core.run(future).unwrap();

        assert_eq!(received_message, sent_message)
    }
}
