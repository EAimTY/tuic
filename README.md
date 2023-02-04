# TUIC

Delicately-TUICed 0-RTT proxy protocol

**Warning: TUIC's [dev](https://github.com/EAimTY/tuic/tree/dev) branch is under heavy development. For end-user, please check out the latest released branch**

## Introduction

TUIC is a proxy protocol focusing on the simplicity. It aims to minimize the additional handshake latency caused by relaying as much as possible

TUIC is originally designed to be used on top of the [QUIC](https://en.wikipedia.org/wiki/QUIC) protocol, but you can use it with any other protocol, e.g. TCP, in theory

When paired with QUIC, TUIC can achieve:

- 0-RTT TCP proxying
- 0-RTT UDP proxying with NAT type [Full Cone](https://www.rfc-editor.org/rfc/rfc3489#section-5)
- 0-RTT authentication
- Two UDP proxying modes:
    - `native`: Having characteristics of native UDP mechanism
    - `quic`: Transferring UDP packets losslessly using QUIC streams
- Fully multiplexed
- All the advantages of QUIC:
    - Bidirectional user-space congestion control
    - Connection migration
    - Optional 0-RTT connection handshake

## Overview

There are 4 crates provided in this repository:

- **[tuic](https://github.com/EAimTY/tuic/tree/dev/tuic)** - Library. The protocol itself, protcol & model abstraction, synchronous / asynchronous marshalling
- **[tuic-quinn](https://github.com/EAimTY/tuic/tree/dev/tuic-quinn)** - Library. A thin layer on top of [quinn](https://github.com/quinn-rs/quinn) to provide functions for TUIC
- **[tuic-server](https://github.com/EAimTY/tuic/tree/dev/tuic-server)** - Binary. Minimalistic TUIC server implementation as a reference, focusing on the simplicity
- **[tuic-client](https://github.com/EAimTY/tuic/tree/dev/tuic-client)** - Binary. Minimalistic TUIC client implementation as a reference, focusing on the simplicity

## License

GNU General Public License v3.0