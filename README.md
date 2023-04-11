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

### Server 
```
Arguments:
    -c, --config <path>     Path to the config file (required)
    -v, --version           Print the version
    -h, --help              Print this help message
```

```json
{
  "server": "[::]:443", 
  "users": {"UUID":"password", "UUID2":"password2"},
  
  "certificate": "path/s.crt",
  "private_key": "path/s.key",
  "disable_sni" : "false",
  "alpn": ["h3"],
  "congestion_control":"cubic",
  "zero_rtt_handshake": false,
  
  "max_idle_time": "15000",
  "authentication_timeout": "1000",
  "max_udp_relay_packet_size": "1500",
 
  "log_level": "info"
}
```

### Client

```json
{
    "relay": {
        "server": "example.com:443",
        "alpn": ["h3"],
        "ip": "IP",
        "uuid": "UUID",
        "password" : "password",
        "udp_relay_mode": "native",
        "zero_rtt_handshake": false,
        "disable_sni": false
    },

    "local": {
        "server": "127.0.0.1:1080"
    },

    "log_level" : "info"
}
```

## GUI Clients

### Android

- [SagerNet](https://sagernet.org/)

### iOS

- [Stash](https://stash.ws/) *

*[Stash](https://stash.ws/) re-implemented the TUIC protocol from scratch, so it didn't preserve the GPL License.

### Windows

- [v2rayN](https://github.com/2dust/v2rayN)


## Overview

There are 4 crates provided in this repository:

- **[tuic](https://github.com/EAimTY/tuic/tree/dev/tuic)** - Library. The protocol itself, protcol & model abstraction, synchronous / asynchronous marshalling
- **[tuic-quinn](https://github.com/EAimTY/tuic/tree/dev/tuic-quinn)** - Library. A thin layer on top of [quinn](https://github.com/quinn-rs/quinn) to provide functions for TUIC
- **[tuic-server](https://github.com/EAimTY/tuic/tree/dev/tuic-server)** - Binary. Minimalistic TUIC server implementation as a reference, focusing on the simplicity
- **[tuic-client](https://github.com/EAimTY/tuic/tree/dev/tuic-client)** - Binary. Minimalistic TUIC client implementation as a reference, focusing on the simplicity

## License

GNU General Public License v3.0