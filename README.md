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

TUIC was designed on the basis of the QUIC protocol from the very beginning. It can make full use of the advantages brought by QUIC.

A typical TUIC connection procedure could like:

First, a QUIC handshake needs to be executed to establish a secure connection between the server and the client. A standard QUIC handshake requires 1 rtt, or 0 rtt if the option `--reduce-rtt` is enabled. [More information about QUIC 0-RTT handshake](https://blog.cloudflare.com/even-faster-connection-establishment-with-quic-0-rtt-resumption/#attack-of-the-clones).

After the QUIC connection is established, the client will immediately send authentication information to the server. Note that the client does not need to wait for the authentication reply from the server, and can directly start sending relay requests to the server. Authentication and relay requests are sent in parallel, so TUIC authentication consumes 0 rtt.

All relay tasks will be multiplexed into a single QUIC connection, so unless the connection is forcibly interrupted or no task within the maximum idle time, negotiating new relay task does not need to go through the process of QUIC handshake and TUIC authentication. Each task is independently flow-controlled, so an error in one task will not cause other tasks to be blocked.

For TCP relay tasks, the client initiates a relay request to the server, and the server attempts to establish a TCP connection to the relay target, informs the client of the result, and then starts relaying. TUIC's TCP relay consumes 1 rtt.

For UDP relay tasks, the client can directly send the UDP packets to the server without sending a request. Similarly, the server can directly return the UDP packets received from the outside to the client. The server and the client synchronize UDP sessions by appending "association ID" to the relayed UDP packet. UDP sessions expire when the client sends a `dissociate` command, or when the QUIC connection is closed.

### UDP Relay Modes

TUIC has 2 UDP relay modes:

- `native` UDP relay mode - using QUIC's datagram to transmit UDP packets. As with native UDP, packets may be lost, but the overhead of the acknowledgment mechanism is omitted. Relayed packets are still encrypted by QUIC in transit.

- `quic` UDP relay mode - transporting UDP packets as QUIC streams. Because of the acknowledgment and retransmission mechanism, UDP packets can guarantee a 100% delivery rate, but have additional transmission overhead as a result. Note that each UDP data packet is transmitted as a separate stream, and the flow controlled separately, so the loss and retransmission of one packet will not cause other packets to be blocked.

### User-space Congestion Control

Since QUIC is implemented over UDP, its congestion control implementation is not limited by platform and operating system. For poor quality network, BBR can be used on both the server and the client to achieve better transmission performance.

## Usage

### Server

```
tuic-server [options] [flags]

Options:
    -p, --port SERVER_PORT
                        (Required) Set the listening port
    -t, --token TOKEN   (Required) Set the token for TUIC authentication
    -c, --cert CERTIFICATE
                        (Required) Set the X.509 certificate. This must be an
                        end-entity certificate
    -k, --priv-key PRIVATE_KEY
                        (Required) Set the private key. Supports PKCS#8 and
                        PKCS#1(RSA) formats
        --authentication-timeout AUTHENTICATION_TIMEOUT
                        Set the maximum time allowed between a QUIC connection
                        established and the TUIC authentication packet
                        received, in milliseconds. Default: 1000ms
        --congestion-controller CONGESTION_CONTROLLER
                        Set the congestion controller. Available: "cubic",
                        "new_reno", "bbr". Default: "cubic"
        --max-udp-packet-size MAX_UDP_PACKET_SIZE
                        Set the maximum UDP packet size. Excess bytes may be
                        discarded. Default: 1536
        --log-level LOG_LEVEL
                        Set the log level. Available: "off", "error", "warn",
                        "info", "debug", "trace". Default: "info"
    -v, --version       Print the version
    -h, --help          Print this help menu
```

### Client

```
tuic-client [options] [flags]

Options:
    -s, --server SERVER (Required) Set the server address. This address must
                        be included in certificate
    -p, --server-port SERVER_PORT
                        (Required) Set the server port
    -t, --token TOKEN   (Required) Set the token for TUIC authentication
    -l, --local-port LOCAL_PORT
                        (Required) Set the listening port for local socks5
                        server
        --server-ip SERVER_IP
                        Set the server IP, for overwriting the DNS lookup
                        result of the server address set in option '-s'
        --socks5-username SOCKS5_USERNAME
                        Set the username for local socks5 server
                        authentication
        --socks5-password SOCKS5_PASSWORD
                        Set the password for local socks5 server
                        authentication
        --allow-external-connection 
                        Allow external connections for local socks5 server
        --cert CERTIFICATE
                        Set the X.509 certificate for QUIC handshake. If not
                        set, native CA roots will be trusted
        --udp-mode UDP_MODE
                        Set the UDP relay mode. Available: "native", "quic".
                        Default: "native"
        --congestion-controller CONGESTION_CONTROLLER
                        Set the congestion controller. Available: "cubic",
                        "new_reno", "bbr". Default: "cubic"
        --reduce-rtt    Enable 0-RTT QUIC handshake
        --max-udp-packet-size MAX_UDP_PACKET_SIZE
                        Set the maximum UDP packet size. Excess bytes may be
                        discarded. Default: 1536
        --log-level LOG_LEVEL
                        Set the log level. Available: "off", "error", "warn",
                        "info", "debug", "trace". Default: "info"
    -v, --version       Print the version
    -h, --help          Print this help menu
```

### Example

A typical TUIC server using BBR as the congestion controller:

```bash
tuic-server \
    -p PORT \
    -t TOKEN \
    -c CERTIFICATE \
    -k PRIVATE_KEY \
    --congestion-controller bbr
```

A typical TUIC client using `quic` udp relay mode, BBR as the congestion controller, with 0-RTT QUIC handshake enabled:

```bash
tuic-client \
    -s SERVER_ADDRESS \
    -p SERVER_PORT
    -t TOKEN \
    -l LOCAL_SOCKS5_SERVER_PORT \
    --udp-mode quic
    --congestion-controller bbr \
    --reduce-rtt
```

## Further Development Plan

- Server multi-user support
- `Bind` Command (similar to socks5's `BIND` command)
- ...

## License
GNU General Public License v3.0
