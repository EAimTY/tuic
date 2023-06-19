# tuic-client

Minimalistic TUIC client implementation as a reference

[![Version](https://img.shields.io/crates/v/tuic-client.svg?style=flat)](https://crates.io/crates/tuic-client)
[![License](https://img.shields.io/crates/l/tuic-client.svg?style=flat)](https://github.com/EAimTY/tuic/blob/dev/LICENSE)

# Overview

The main goal of this TUIC client implementation is not to provide a full-featured, production-ready TUIC client, but to provide a minimal reference for the TUIC protocol client implementation.

This implementation only contains the most basic requirements of a functional TUIC protocol client. If you are looking for features like HTTP-inbound, load-balance, etc., try other implementations, or implement them yourself.

## Usage

Download the latest binary from [releases](https://github.com/EAimTY/tuic/releases).

Or install from [crates.io](https://crates.io/crates/tuic-client):

```bash
cargo install tuic-client
```

Run the TUIC client with configuration file:

```bash
tuic-client -c PATH/TO/CONFIG
```

## Configuration

```json5
{
    // Settings for the outbound TUIC proxy
    "relay": {
        // Set the TUIC proxy server address
        // Format: "HOST:PORT"
        // The HOST must be a common name in the certificate
        // If the "ip" field in the "relay" section is not set, the HOST is also used for DNS resolving
        "server": "example.com:443",

        // Set the user UUID
        "uuid": "00000000-0000-0000-0000-000000000000",

        // Set the user password
        "password": "PASSWORD",

        // Optional. The IP address of the TUIC proxy server, for overriding DNS resolving
        // If not set, the HOST in the "server" field is used for DNS resolving
        "ip": "127.0.0.1",

        // Optional. A list of certificates for TLS handshake
        // System native certificates are also loaded by default
        // When using self-signed certificates, the full certificate chain must be provided
        "certificates": ["PATH/TO/CERTIFICATE_1", "PATH/TO/CERTIFICATE_2"],

        // Optional. Set the UDP packet relay mode
        // Can be:
        // - "native": native UDP characteristics
        // - "quic": lossless UDP relay using QUIC streams, additional overhead is introduced
        // Default: "native"
        "udp_relay_mode": "native",

        // Optional. Congestion control algorithm, available options:
        // "cubic", "new_reno", "bbr"
        // Default: "cubic"
        "congestion_control": "cubic",

        // Optional. Application layer protocol negotiation
        // Default being empty (no ALPN)
        "alpn": ["h3", "spdy/3.1"],

        // Optional. Enable 0-RTT QUIC connection handshake on the client side
        // This is not impacting much on the performance, as the protocol is fully multiplexed
        // WARNING: Disabling this is highly recommended, as it is vulnerable to replay attacks. See https://blog.cloudflare.com/even-faster-connection-establishment-with-quic-0-rtt-resumption/#attack-of-the-clones
        // Default: false
        "zero_rtt_handshake": false,

        // Optional. Disable SNI (Server Name Indication) in TLS handshake
        // The server name used in SNI is the same as the HOST in the "server" field
        // Default: false
        "disable_sni": false,

        // Optional. Set the timeout for establishing a connection to the TUIC proxy server
        // Default: "8s"
        "timeout": "8s",

        // Optional. Set the interval for sending heartbeat packets for keeping the connection alive
        // Default: "3s"
        "heartbeat": "3s",

        // Optional. Disable loading system native certificates
        // Default: false
        "disable_native_certs": false,

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
        "gc_lifetime": "15s"
    },

    // Settings for the local inbound socks5 server
    "local": {
        // Local socks5 server address
        "server": "[::]:1080",

        // Optional. Set the username for socks5 authentication
        "username": "USERNAME",

        // Optional. Set the password for socks5 authentication
        "password": "PASSWORD",
        
        // Optional. Set if the listening socket should be dual-stack
        // If this option is not set, the socket behavior is platform dependent
        "dual_stack": true,

        // Optional. Maximum packet size the socks5 server can receive from external, in bytes
        // Default: 1500
        "max_packet_size": 1500
    },

    // Optional. Set the log level
    // Default: "warn"
    "log_level": "warn"
}
```

## License

GNU General Public License v3.0
