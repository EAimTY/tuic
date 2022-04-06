# TUIC

Delicately-TUICed high-performance proxy built on top of the [QUIC](https://en.wikipedia.org/wiki/QUIC) protocol

## Features

- 1-RTT TCP relaying
- 0-RTT UDP relaying with NAT type [FullCone](https://www.rfc-editor.org/rfc/rfc3489#section-5)
- Two UDP relay modes: `native` (native UDP mechanisms) and `quic` (100% delivery rate)
- User-space congestion control (BBR, New Reno and CUBIC)
- Multiplexing all tasks into a single QUIC connection (tasks are separately flow controlled)
- Smooth session transfer on network switching
- Paralleled 0-RTT authentication
- Support QUIC 0-RTT handshake

## Design

TUIC was designed on the basis of the QUIC protocol from the very beginning. It can make full use of the advantages brought by QUIC. You can find more information about the TUIC protocol [here](https://github.com/EAimTY/tuic/tree/master/protocol).

TUIC's goal is to provide a highly usable and efficient proxy even on poor quality network, and minimize the handshake latency as much as possible.

### Multiplexing

TUIC multiplexes all tasks into a single QUIC connection by using QUIC's stream mechanism. This means that unless the QUIC connection is forcibly interrupted or no task within the maximum idle time, negotiating new relay task does not need to go through the process of QUIC handshake and TUIC authentication.

### UDP Relaying

TUIC has 2 UDP relay modes:

- `native` - using QUIC's datagram to transmit UDP packets. As with native UDP, packets may be lost, but the overhead of the acknowledgment mechanism is omitted. Relayed packets are still encrypted by QUIC.

- `quic` - transporting UDP packets as QUIC streams. Because of the acknowledgment and retransmission mechanism, UDP packets can guarantee a 100% delivery rate, but have additional transmission overhead as a result. Note that each UDP data packet is transmitted as a separate stream, and the flow controlled separately, so the loss and retransmission of one packet will not cause other packets to be blocked.

### User-space Congestion Control

Since QUIC is implemented over UDP, its congestion control implementation is not limited by platform and operating system. For poor quality network, [BBR algorithm](https://en.wikipedia.org/wiki/TCP_congestion_control#TCP_BBR) can be used on both the server and the client to achieve better transmission performance.

### Security and Detectability

As mentioned above, TUIC is based on the QUIC protocol, which uses TLS to encrypt data. TUIC protocol itself does not provide any security, but the QUIC protocol provides a strong security guarantee. TUIC also supports QUIC's 0-RTT handshake, but it came with a cost of weakened security, [read more about QUIC 0-RTT handshake](https://blog.cloudflare.com/even-faster-connection-establishment-with-quic-0-rtt-resumption/#attack-of-the-clones).

## Usage

TUIC depends on [rustls](https://github.com/rustls/rustls), which uses [ring](https://github.com/briansmith/ring) for implementing the cryptography in TLS. As a result, TUIC only runs on platforms supported by ring. At the time of writing this means x86, x86-64, armv7, and aarch64.

You can find pre-compiled binaries in the latest [releases](https://github.com/EAimTY/tuic/releases/latest).

### Server

```
tuic-server

Options:
    -c, --config CONFIG_FILE
                        Read configuration from a file. Note that command line
                        arguments will override the configuration file
        --port SERVER_PORT
                        Set the server listening port
        --token TOKEN   Set the token for TUIC authentication
        --certificate CERTIFICATE
                        Set the X.509 certificate. This must be an end-entity
                        certificate
        --private-key PRIVATE_KEY
                        Set the certificate private key
        --congestion-controller CONGESTION_CONTROLLER
                        Set the congestion control algorithm. Available:
                        "cubic", "new_reno", "bbr". Default: "cubic"
        --max-idle-time MAX_IDLE_TIME
                        Set the maximum idle time for connections, in
                        milliseconds. The true idle timeout is the minimum of
                        this and the client's one. Default: 15000
        --authentication-timeout AUTHENTICATION_TIMEOUT
                        Set the maximum time allowed between a QUIC connection
                        established and the TUIC authentication packet
                        received, in milliseconds. Default: 1000
        --max-udp-packet-size MAX_UDP_PACKET_SIZE
                        Set the maximum UDP packet size, in bytes. Excess
                        bytes may be discarded. Default: 1536
        --enable-ipv6   Enable IPv6 support
        --log-level LOG_LEVEL
                        Set the log level. Available: "off", "error", "warn",
                        "info", "debug", "trace". Default: "info"
    -v, --version       Print the version
    -h, --help          Print this help menu
```

The configuration file is in the JSON format:

```json
{
    "port": 443,
    "token": "TOKEN",
    "certificate": "/PATH/TO/CERT",
    "private_key": "/PATH/TO/PRIV_KEY",

    "congestion_controller": "cubic",
    "max_idle_time": 15000,
    "authentication_timeout": 1000,
    "max_udp_packet_size": 1536,
    "enable_ipv6": false,
    "log_level": "info"
}
```

Fields `congestion_controller`, `max_idle_time`, `authentication_timeout`, `max_udp_packet_size`, `enable_ipv6` and `log_level` are optional.

Note that command line arguments can override the configuration file.

### Client

```
tuic-client

Options:
    -c, --config CONFIG_FILE
                        Read configuration from a file. Note that command line
                        arguments will override the configuration file
        --server SERVER Set the server address. This address must be included
                        in the certificate
        --server-port SERVER_PORT
                        Set the server port
        --server-ip SERVER_IP
                        Set the server IP, for overwriting the DNS lookup
                        result of the server address set in option 'server'
        --token TOKEN   Set the token for TUIC authentication
        --certificate CERTIFICATE
                        Set the X.509 certificate for QUIC handshake. If not
                        set, native CA roots will be trusted
        --udp-mode UDP_MODE
                        Set the UDP relay mode. Available: "native", "quic".
                        Default: "native"
        --congestion-controller CONGESTION_CONTROLLER
                        Set the congestion control algorithm. Available:
                        "cubic", "new_reno", "bbr". Default: "cubic"
        --max-idle-time MAX_IDLE_TIME
                        Set the maximum idle time for connections, in
                        milliseconds. The true idle timeout is the minimum of
                        this and the client's one. Default: 15000
        --heartbeat-interval HEARTBEAT_INTERVAL
                        Set the heartbeat interval, in milliseconds. This
                        ensures that the QUIC connection is not closed when
                        there are relay tasks but no data transfer. Default:
                        10000
        --reduce-rtt    Enable 0-RTT QUIC handshake
        --local-port LOCAL_PORT
                        Set the listening port for the local socks5 server
        --local-username LOCAL_USERNAME
                        Set the username for the local socks5 server
                        authentication
        --local-password LOCAL_PASSWORD
                        Set the password for the local socks5 server
                        authentication
        --allow-external-connection 
                        Allow external connections for local socks5 server
        --enable-ipv6   Enable IPv6 support
        --max-udp-packet-size MAX_UDP_PACKET_SIZE
                        Set the maximum UDP packet size, in bytes. Excess
                        bytes may be discarded. Default: 1536
        --log-level LOG_LEVEL
                        Set the log level. Available: "off", "error", "warn",
                        "info", "debug", "trace". Default: "info"
    -v, --version       Print the version
    -h, --help          Print this help menu
```

The configuration file is in the JSON format:

```json
{
    "relay": {
        "server": "SERVER",
        "port": 443,
        "token": "TOKEN",

        "ip": "SERVER_IP",
        "certificate": "/PATH/TO/CERT",
        "udp_mode": "native",
        "congestion_controller": "cubic",
        "max_idle_time": 15000,
        "heartbeat_interval": 10000,
        "reduce_rtt": false
    },
    "local": {
        "port": 1080,

        "username": "SOCKS5_USERNAME",
        "password": "SOCKS5_PASSWORD",
        "allow_external_connection": false
    },
    "enable_ipv6": false,
    "log_level": "info"
}
```

Fields `ip`, `certificate`, `udp_mode`, `congestion_controller`, `max_idle_time`, `heartbeat_interval` ,`reduce_rtt` in `relay` section ,fields `username`, `password`, `allow_external_connection` in `local` section and fields `enable_ipv6` and `log_level` are optional.

Note that command line arguments can override the configuration file.

## License
GNU General Public License v3.0
