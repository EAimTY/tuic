# tuic-server

Minimalistic TUIC server implementation as a reference

[![Version](https://img.shields.io/crates/v/tuic-server.svg?style=flat)](https://crates.io/crates/tuic-server)
[![License](https://img.shields.io/crates/l/tuic-server.svg?style=flat)](https://github.com/EAimTY/tuic/blob/dev/LICENSE)

# Overview

The main goal of this TUIC server implementation is not to provide a full-featured, production-ready TUIC server, but to provide a minimal reference for the TUIC protocol server implementation.

This implementation only contains the most basic requirements of a functional TUIC protocol server. If you are looking for features like outbound-control, DNS-caching, etc., try other implementations, or implement them yourself.

## Usage

Download the latest binary from [releases](https://github.com/EAimTY/tuic/releases).

Or install from [crates.io](https://crates.io/crates/tuic-server):

```bash
cargo install tuic-server
```

Run the TUIC server with configuration file:

```bash
tuic-server -c PATH/TO/CONFIG
```

Or with Docker

```bash
docker run --name tuic-server \
  --restart always \
  —-network host \
  -v /PATH/TO/CONFIG:/etc/tuic/config.json \
  -v /PATH/TO/CERTIFICATE:PATH/TO/CERTIFICATE \
  -v /PATH/TO/PRIVATE_KEY:PATH/TO/PRIVATE_KEY \
  -dit ghcr.io/eaimty/tuic-server:dev
```

## Configuration

```json5
{
    // The socket address to listen on
    "server": "[::]:443",

    // User list, contains user UUID and password
    "users": {
        "00000000-0000-0000-0000-000000000000": "PASSWORD_0",
        "00000000-0000-0000-0000-000000000001": "PASSWORD_1"
    },

    // The path to the certificate file
    "certificate": "PATH/TO/CERTIFICATE",

    // The path to the private key file
    "private_key": "PATH/TO/PRIVATE_KEY",

    // Optional. Congestion control algorithm, available options:
    // "cubic", "new_reno", "bbr"
    // Default: "cubic"
    "congestion_control": "cubic",

    // Optional. Application layer protocol negotiation
    // Default being empty (no ALPN)
    "alpn": ["h3", "spdy/3.1"],

    // Optional. If the server should create separate UDP sockets for relaying IPv6 UDP packets
    // Default: true
    "udp_relay_ipv6": true,

    // Optional. Enable 0-RTT QUIC connection handshake on the server side
    // This is not impacting much on the performance, as the protocol is fully multiplexed
    // WARNING: Disabling this is highly recommended, as it is vulnerable to replay attacks. See https://blog.cloudflare.com/even-faster-connection-establishment-with-quic-0-rtt-resumption/#attack-of-the-clones
    // Default: false
    "zero_rtt_handshake": false,

    // Optional. Set if the listening socket should be dual-stack
    // If this option is not set, the socket behavior is platform dependent
    "dual_stack": true,

    // Optional. How long the server should wait for the client to send the authentication command
    // Default: 3s
    "auth_timeout": "3s",

    // Optional. Maximum duration server expects for task negotiation
    // Default: 3s
    "task_negotiation_timeout": "3s",

    // Optional. How long the server should wait before closing an idle connection
    // Default: 10s
    "max_idle_time": "10s",

    // Optional. Maximum packet size the server can receive from outbound UDP sockets, in bytes
    // Default: 1500
    "max_external_packet_size": 1500,

    // Optional. Maximum number of bytes to transmit to a peer without acknowledgment
    // Should be set to at least the expected connection latency multiplied by the maximum desired throughput
    // Default: 8MiB * 2
    "send_window": 16777216,

    // Optional. Maximum number of bytes the peer may transmit without acknowledgement on any one stream before becoming blocked
    // Should be set to at least the expected connection latency multiplied by the maximum desired throughput
    // Default: 8MiB
    "receive_window": 8388608,

    // Optional. Interval between UDP packet fragment garbage collection
    // Default: 3s
    "gc_interval": "3s",

    // Optional. How long the server should keep a UDP packet fragment. Outdated fragments will be dropped
    // Default: 15s
    "gc_lifetime": "15s",

    // Optional. Set the log level
    // Default: "warn"
    "log_level": "warn"
}
```

## License

GNU General Public License v3.0
