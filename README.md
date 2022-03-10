# TUIC

Delicately-TUIC'ed high-performance proxy built on top of the [QUIC](https://en.wikipedia.org/wiki/QUIC) protocol

## Features

- 1-RTT TCP relaying (with client `--reduce-rtt` option flag)
- Multiplexing all tasks into a single QUIC connection (tasks are still separately flow controlled)
- User-space congestion controller
- Smooth session transfer on network switching
- Paralleled 0-RTT authentication

## Design

TUIC was designed on the basis of QUIC protocol from the beginning, so it can make full use of the advantages brought by QUIC protocol.

A typical TCP handshake requires at least 1 rtt. After the TCP connection is established, it still needs to negotiate and establish a secure session layer (e.g. TLS) before the application layer usage. In contrast, QUIC supports 0-RTT handshake mode*, which means that it can directly establish a secure connection without increasing round-trip delay. With 0-RTT mode enabled, TUIC can achieve 1-RTT TCP relaying (0 rtt for QUIC handshake, 1 rtt for TUIC protocol), 2x faster than [Shadowsocks](https://github.com/shadowsocks/shadowsocks-rust).

TUIC also took advantages of other QUIC features, like multiplex, user-space congestion controller and smooth session transfer on network switching. TUIC can use BBR congestion control algorithm on both server and client side to improve connection quality.

*QUIC 0-RTT mode does came with the cost of weakened security, but it is still secure enough for non-sensitive data. [More information](https://blog.cloudflare.com/even-faster-connection-establishment-with-quic-0-rtt-resumption/#attack-of-the-clones).

## Usage

TUIC has seperated server and client binary file

### Server

```
Options:
    -p, --port SERVER_PORT
                        Set the listening port(Required)
    -t, --token TOKEN   Set the TUIC token for the authentication(Required)
    -c, --cert CERTIFICATE
                        Set the certificate for QUIC handshake(Required)
    -k, --priv-key PRIVATE_KEY
                        Set the private key for QUIC handshake(Required)
        --congestion-controller CONGESTION_CONTROLLER
                        Set the congestion controller. Available: "cubic"
                        (default), "new_reno", "bbr"
    -v, --version       Print the version
    -h, --help          Print this help menu
```

### Client

```
Options:
    -s, --server SERVER Set the server address. This address is supposed to be
                        in the certificate(Required)
    -p, --server-port SERVER_PORT
                        Set the server port(Required)
    -t, --token TOKEN   Set the TUIC token for the server
                        authentication(Required)
    -l, --local-port LOCAL_PORT
                        Set the listening port of the local socks5
                        server(Required)
        --server-ip SERVER_IP
                        Set the server IP, for overwriting the DNS lookup
                        result of the server address
        --socks5-username SOCKS5_USERNAME
                        Set the username of the local socks5 server
                        authentication
        --socks5-password SOCKS5_PASSWORD
                        Set the password of the local socks5 server
                        authentication
        --cert CERTIFICATE
                        Set the custom certificate for QUIC handshake. If not
                        set, the platform's native roots will be trusted
        --congestion-controller CONGESTION_CONTROLLER
                        Set the congestion controller. Available: "cubic"
                        (default), "new_reno", "bbr"
        --reduce-rtt    Enable 0-RTT for QUIC handshake at the cost of
                        weakened security
        --allow-external-connection 
                        Allow external connections to the local socks5 server
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

A typical TUIC client using BBR as the congestion controller, with 0-RTT QUIC handshake enabled:

```bash
tuic-client \
    -s SERVER_ADDRESS \
    -p SERVER_PORT
    -t TOKEN \
    -l LOCAL_SOCKS5_SERVER_PORT \
    --congestion-controller bbr \
    --reduce-rtt
```

## Further Development Plan

TUIC is usable, but still lacking a lot of features.
Features below are under development:

- Two 0-RTT UDP relay mode: UDP over QUIC (UDP packets parallelly sends in a exclusive stream) and unreliable UDP
- Multi user support
- `Bind` Command
- ...

## License
GNU General Public License v3.0
