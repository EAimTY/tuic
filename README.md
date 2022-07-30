# TUIC

Delicately-TUICed high-performance proxy built on top of the [QUIC](https://en.wikipedia.org/wiki/QUIC) protocol

**TUIC's goal is to minimize the handshake latency as much as possible**

## Features

- 1-RTT TCP relaying
- 0-RTT UDP relaying with [Full Cone NAT](https://www.rfc-editor.org/rfc/rfc3489#section-5)
- Two UDP relay modes: `native` (native UDP mechanisms) and `quic` (100% delivery rate)
- Bidirectional user-space congestion control (BBR, New Reno and CUBIC)
- Multiplexing all tasks into a single QUIC connection (tasks are separately flow controlled)
- Smooth session transfer on network switching
- Paralleled 0-RTT authentication
- Optional QUIC 0-RTT handshake

## Design

TUIC was designed on the basis of the QUIC protocol from the very beginning. It can make full use of the advantages brought by QUIC. You can find more information about the TUIC protocol [here](https://github.com/EAimTY/tuic/tree/master/protocol).

### Multiplexing

TUIC multiplexes all tasks into a single QUIC connection using QUIC's multi-streams mechanism. This means that unless the QUIC connection is forcibly interrupted or no task within the maximum idle time, negotiating new relay task does not need to go through the process of QUIC handshake and TUIC authentication.

### UDP Relaying

TUIC has 2 UDP relay modes:

- `native` - using QUIC's datagram to transmit UDP packets. As with native UDP, packets may be lost, but the overhead of the acknowledgment mechanism is omitted. Relayed packets are still encrypted by QUIC.

- `quic` - transporting UDP packets as QUIC streams. Because of the acknowledgment and retransmission mechanism, UDP packets can guarantee a 100% delivery rate, but have additional transmission overhead as a result. Note that each UDP data packet is transmitted as a separate stream, and the flow controlled separately, so the loss and retransmission of one packet will not cause other packets to be blocked. This mode can be used to transmit UDP packets larger than the MTU of the underlying network.

### Bidirectional User-space Congestion Control

Since QUIC is implemented over UDP, its congestion control implementation is not limited by platform and operating system. For poor quality network, [BBR algorithm](https://en.wikipedia.org/wiki/TCP_congestion_control#TCP_BBR) can be used on both the server and the client to achieve better transmission performance.

### Security

As mentioned above, TUIC is based on the QUIC protocol, which uses TLS to encrypt data. TUIC protocol itself does not provide any security, but the QUIC protocol provides a strong security guarantee. TUIC also supports QUIC's 0-RTT handshake, but it came with a cost of weakened security, [read more about QUIC 0-RTT handshake](https://blog.cloudflare.com/even-faster-connection-establishment-with-quic-0-rtt-resumption/#attack-of-the-clones).

## Usage

TUIC depends on [rustls](https://github.com/rustls/rustls), which uses [ring](https://github.com/briansmith/ring) for implementing the cryptography in TLS. As a result, TUIC only runs on platforms supported by ring. At the time of writing this means x86, x86-64, armv7, and aarch64.

You can find pre-compiled binaries in the latest [releases](https://github.com/EAimTY/tuic/releases).

### Server

```
tuic-server

Options:
    -c, --config CONFIG_FILE
                        Read configuration from a file. Note that command line
                        arguments will override the configuration file
        --port SERVER_PORT
                        Set the server listening port
        --token TOKEN   Set the token for TUIC authentication. This option can
                        be used multiple times to set multiple tokens.
        --certificate CERTIFICATE
                        Set the X.509 certificate. This must be an end-entity
                        certificate
        --private-key PRIVATE_KEY
                        Set the certificate private key
        --ip IP         Set the server listening IP. Default: 0.0.0.0
        --congestion-controller CONGESTION_CONTROLLER
                        Set the congestion control algorithm. Available:
                        "cubic", "new_reno", "bbr". Default: "cubic"
        --max-idle-time MAX_IDLE_TIME
                        Set the maximum idle time for QUIC connections, in
                        milliseconds. Default: 15000
        --authentication-timeout AUTHENTICATION_TIMEOUT
                        Set the maximum time allowed between a QUIC connection
                        established and the TUIC authentication packet
                        received, in milliseconds. Default: 1000
        --alpn ALPN_PROTOCOL
                        Set ALPN protocols that the server accepts. This
                        option can be used multiple times to set multiple ALPN
                        protocols. If not set, the server will not check ALPN
                        at all
        --max-udp-relay-packet-size MAX_UDP_RELAY_PACKET_SIZE
                        UDP relay mode QUIC can transmit UDP packets larger
                        than the MTU. Set this to a higher value allows
                        outbound to receive larger UDP packet. Default: 1500
        --log-level LOG_LEVEL
                        Set the log level. Available: "off", "error", "warn",
                        "info", "debug", "trace". Default: "info"
    -v, --version       Print the version
    -h, --help          Print this help menu
```

The configuration file is in JSON format:

```json
{
    "port": 443,
    "token": ["TOKEN0", "TOKEN1"],
    "certificate": "/PATH/TO/CERT",
    "private_key": "/PATH/TO/PRIV_KEY",

    "ip": "0.0.0.0",
    "congestion_controller": "cubic",
    "max_idle_time": 15000,
    "authentication_timeout": 1000,
    "alpn": ["h3"],
    "max_udp_relay_packet_size": 1500,
    "log_level": "info"
}
```

Fields `port`, `token`, `certificate`, `private_key` are required. Other fields are optional and can be deleted to fall-back the default value.

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
        --token TOKEN   Set the token for TUIC authentication
        --server-ip SERVER_IP
                        Set the server IP, for overwriting the DNS lookup
                        result of the server address set in option 'server'
        --certificate CERTIFICATE
                        Set custom X.509 certificate alongside native CA roots
                        for the QUIC handshake. This option can be used
                        multiple times to set multiple certificates
        --udp-relay-mode UDP_MODE
                        Set the UDP relay mode. Available: "native", "quic".
                        Default: "native"
        --congestion-controller CONGESTION_CONTROLLER
                        Set the congestion control algorithm. Available:
                        "cubic", "new_reno", "bbr". Default: "cubic"
        --heartbeat-interval HEARTBEAT_INTERVAL
                        Set the heartbeat interval to ensures that the QUIC
                        connection is not closed when there are relay tasks
                        but no data transfer, in milliseconds. This value
                        needs to be smaller than the maximum idle time set at
                        the server side. Default: 10000
        --alpn ALPN_PROTOCOL
                        Set ALPN protocols included in the TLS client hello.
                        This option can be used multiple times to set multiple
                        ALPN protocols. If not set, no ALPN extension will be
                        sent
        --disable-sni   Not sending the Server Name Indication (SNI) extension
                        during the client TLS handshake
        --reduce-rtt    Enable 0-RTT QUIC handshake
        --request-timeout REQUEST_TIMEOUT
                        Set the timeout for negotiating tasks between client
                        and the server, in milliseconds. Default: 8000
        --max-udp-relay-packet-size MAX_UDP_RELAY_PACKET_SIZE
                        UDP relay mode QUIC can transmit UDP packets larger
                        than the MTU. Set this to a higher value allows
                        inbound to receive larger UDP packet. Default: 1500
        --local-port LOCAL_PORT
                        Set the listening port for the local socks5 server
        --local-ip LOCAL_IP
                        Set the listening IP for the local socks5 server. Note
                        that the sock5 server socket will be a dual-stack
                        socket if it is IPv6. Default: "127.0.0.1"
        --local-username LOCAL_USERNAME
                        Set the username for the local socks5 server
                        authentication
        --local-password LOCAL_PASSWORD
                        Set the password for the local socks5 server
                        authentication
        --log-level LOG_LEVEL
                        Set the log level. Available: "off", "error", "warn",
                        "info", "debug", "trace". Default: "info"
    -v, --version       Print the version
    -h, --help          Print this help menu
```

The configuration file is in JSON format:

```json
{
    "relay": {
        "server": "SERVER",
        "port": 443,
        "token": "TOKEN",

        "ip": "SERVER_IP",
        "certificates": ["/PATH/TO/CERT"],
        "udp_relay_mode": "native",
        "congestion_controller": "cubic",
        "heartbeat_interval": 10000,
        "alpn": ["h3"],
        "disable_sni": false,
        "reduce_rtt": false,
        "request_timeout": 8000,
        "max_udp_relay_packet_size": 1500
    },
    "local": {
        "port": 1080,

        "ip": "127.0.0.1",
        "username": "SOCKS5_USERNAME",
        "password": "SOCKS5_PASSWORD"
    },
    "log_level": "info"
}
```

Fields `server`, `token` and `port` in both sections are required. Other fields are optional and can be deleted to fall-back the default value.

Note that command line arguments can override the configuration file.

## GUI Clients

### Android

- [SagerNet](https://sagernet.org/)

### iOS

- [Stash](https://stash.ws/) *

*[Stash](https://stash.ws/) re-implemented the TUIC protocol from scratch, so it didn't preserve the GPL License.

### Windows

- [v2rayN](https://github.com/2dust/v2rayN)

## FAQ

### What are the advantages of TUIC over other proxy protocols / implementions?

As mentioned before, TUIC's goal is to minimize the handshake latency as much as possible. Thus, the core of the TUIC protocol is to reduce the additional round trip time added by the relay. For TCP relaying, TUIC only adds a single round trip between the TUIC server and the TUIC client - half of a typical TCP-based proxy would require. TUIC also has a unique UDP relaying mechanism. It achieves 0-RTT UDP relaying by syncing UDP relay sessions implicitly between the server and the client.

Low handshake latency means faster connection establishment and UDP packet delay time. TUIC also supports both UDP over streams and UDP over datagrams for UDP relaying. All of these makes TUIC one of the most efficient proxy protocol for UDP relaying.

### Why my TUIC is slower than other proxy protocols / implementions?

For an Internet connection, fast / slow is defined by both:

- Handshake latency
- Bandwidth

They are equally important. For the first case, TUIC can be one of the best solution right now. You can directly feel it from things like the speed of opening a web page in your browser. For the second case, TUIC may be a bit slower than other TCP-based proxy protocols due to ISPs' QoS, but TUIC's bandwidth can still be competitive in most scenario.

### How can I listen both IPv4 and IPv6 on TUIC server / TUIC client's socks5 server?

TUIC always constructs an IPv6 listener as a dual-stack socket. If you need to listen on both IPv4 and IPv6, you can set the bind IP to the unspecified IPv6 address `::`.

### Why TUIC client doesn't support other inbound / advanced route settings?

Since there are already many great proxy convert / distribute solutions, there really is no need for me to reimplement those again. If you need those functions, the best choice to chain a V2Ray layer in front of the TUIC client. For a typical network program, there is basically no performance cost for local relaying.

### Why TUIC client is not able to convert the first connection into 0-RTT

It is totally fine and designed to be like that.

> The basic idea behind 0-RTT connection resumption is that if the client and server had previously established a TLS connection between each other, they can use information cached from that session to establish a new one without having to negotiate the connection’s parameters from scratch. Notably this allows the client to compute the private encryption keys required to protect application data before even talking to the server.
>
> *--[Even faster connection establishment with QUIC 0-RTT resumption](https://blog.cloudflare.com/even-faster-connection-establishment-with-quic-0-rtt-resumption) - Cloudflare*

When the client program starts, trying to convert the very first connection to 0-RTT will always fail because the client has no server-related information yet. This connection handshake  will fall-back to the regular 1-RTT one.

Once the client caches server information from the first connection, any subsequent connection will be convert into a 0-RTT one. That is why you only see this warning message once just after starting the client.

Therefore, you can safely ignore this warn.

## License

GNU General Public License v3.0
