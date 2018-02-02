# Lz Shared UDP

Functionality for a shared [`tokio_core::net::UdpSocket`](https://docs.rs/tokio-core/0.1.12/tokio_core/net/struct.UdpSocket.html). 

[![Build Status](https://travis-ci.org/Lukazoid/lz_shared_udp.svg?branch=master)](https://travis-ci.org/Lukazoid/lz_shared_udp)

[Documentation](https://docs.rs/lz_shared_udp)

## Features 
 - An implementation of [`tokio_core::net::UdpSocket::framed<C: UdpCodec>(self)`](https://docs.rs/tokio-core/0.1.12/tokio_core/net/struct.UdpSocket.html#method.framed) which doesn't take ownership of the `UdpSocket` (this may also be used with `Rc<UdpSocket>` and `Arc<UdpSocket>`).


# License

This project is licensed under the MIT License ([LICENSE](LICENSE) or http://opensource.org/licenses/MIT).