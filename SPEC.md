# TUIC Protocol

## Version

`0x05`

## Overview

The TUIC protocol relies on a multiplex-able TLS-encrypted stream. All relaying tasks are negotiated by the `Header` in `Command`s.

The protocol doesn't care about the underlying transport. However, it is mainly designed to be used with [QUIC](https://en.wikipedia.org/wiki/QUIC). See [Protocol Flow](#protocol-flow) for detailed mechanism.

All fields are in Big Endian unless otherwise noted.

## Command

```plain
+-----+------+----------+
| VER | TYPE |   OPT    |
+-----+------+----------+
|  1  |  1   | Variable |
+-----+------+----------+
```

where:

- `VER` - the TUIC protocol version
- `TYPE` - command type
- `OPT` - command type specific data

### Command Types

There are five types of command:

- `0x00` - `Authenticate` - for authenticating the multiplexed stream
- `0x01` - `Connect` - for establishing a TCP relay
- `0x02` - `Packet` - for relaying (fragmented part of) a UDP packet
- `0x03` - `Dissociate` - for terminating a UDP relaying session
- `0x04` - `Heartbeat` - for keeping the QUIC connection alive

Command `Connect` and `Packet` carry payload (stream / packet fragment)

### Command Type Specific Data

#### `Authenticate`

```plain
+------+-------+
| UUID | TOKEN |
+------+-------+
|  16  |  32   |
+------+-------+
```

where:

- `UUID` - client UUID
- `TOKEN` - client token. The client raw password is hashed into a 256-bit long token using [TLS Keying Material Exporter](https://www.rfc-editor.org/rfc/rfc5705) on current TLS session. While exporting, the `label` should be the client UUID and the `context` should be the raw password.

#### `Connect`

```plain
+----------+
|   ADDR   |
+----------+
| Variable |
+----------+
```

where:

- `ADDR` - target address. See [Address](#address)

#### `Packet`

```plain
+----------+--------+------------+---------+------+----------+
| ASSOC_ID | PKT_ID | FRAG_TOTAL | FRAG_ID | SIZE |   ADDR   |
+----------+--------+------------+---------+------+----------+
|    2     |   2    |     1      |    1    |  2   | Variable |
+----------+--------+------------+---------+------+----------+
```

where:

- `ASSOC_ID` - UDP relay session ID. See [UDP relaying](#udp-relaying)
- `PKT_ID` - UDP packet ID. See [UDP relaying](#udp-relaying)
- `FRAG_TOTAL` - total number of fragments of the UDP packet
- `FRAG_ID` - fragment ID of the UDP packet
- `SIZE` - length of the (fragmented) UDP packet
- `ADDR` - target (from client) or source (from server) address. See [Address](#address)

#### `Dissociate`

```plain
+----------+
| ASSOC_ID |
+----------+
|    2     |
+----------+
```

where:

- `ASSOC_ID` - UDP relay session ID. See [UDP relaying](#udp-relaying)

#### `Heartbeat`

```plain
+-+
| |
+-+
| |
+-+
```

### `Address`

`Address` is a variable-length field that encodes the network address

```plain
+------+----------+----------+
| TYPE |   ADDR   |   PORT   |
+------+----------+----------+
|  1   | Variable |    2     |
+------+----------+----------+
```

where:

- `TYPE` - the address type
- `ADDR` - the address
- `PORT` - the port

The address type can be one of the following:

- `0xff`: None
- `0x00`: Fully-qualified domain name (the first byte indicates the length of the domain name)
- `0x01`: IPv4 address
- `0x02`: IPv6 address

Address type `None` is used in `Packet` commands that is not the first fragment of a UDP packet.

The port number is encoded in 2 bytes after the Domain name / IP address.

## Protocol Flow

This section describes the protocol flow in detail with QUIC as the underlying transport.

The TUIC protocol doesn't care about how the underlying transport is managed. It can even be integrated into other existing services, such as HTTP/3.

Here is a typical flow of the TUIC protocol on a QUIC connection:

### Authentication

The client opens a `unidirectional_stream` and sends a `Authenticate` command. This procedure can be parallelized with other commands (relaying tasks).

The server receives the `Authenticate` command and verifies the token. If the token is valid, the connection is authenticated and ready for other relaying tasks.

If the server receives other commands before the `Authenticate` command, it should only accept the command header part and pause. After the connection is authenticated, the server should resume all the paused tasks.

### TCP relaying

Command `Connect` is used for initializing a TCP relay.

The client opens a `bidirectional_stream` and sends a `Connect` command. After the command header transmission is completed, the client can start using the stream for TCP relaying, no need to wait for the server's response (server will never respond, actually).

The server receives the `Connect` command and opens a TCP stream to the target address. After the stream is established, the server can start relaying data between the TCP stream and the `bidirectional_stream`.

### UDP relaying

TUIC achieves 0-RTT Full Cone UDP forwarding by syncing UDP session ID (associate ID) between the client and the server.

Both the client and the server should create a UDP session table for each QUIC connection, mapping every associate ID to an associated UDP socket.

The associate ID is a 16-bit unsigned integer generated by the client. If the client wants to send UDP packets using the same socket of the server, the attached associate ID in the `Packet` command should be the same.

When receiving a `Packet` command, the server should check whether the attached associate ID is already associated with a UDP socket. If not, the server should allocate a UDP socket for the associate ID. The server will use this UDP socket to send UDP packets requested by the client, and accepting UDP packets from any destination at the same time, prefixing them with the `Packet` command header then sends back to the client.

A UDP packet can be fragmented into multiple `Packet` commands. Field `PKT_ID`, `FRAG_TOTAL` and `FRAG_ID` are used to identify and reassemble the fragmented UDP packets.

As a client, a `Packet` can be sent through:

- QUIC `unidirectional_stream` (UDP relay mode quic)
- QUIC `datagram` (UDP relay mode native)

When the server receives the first `Packet` from an UDP relay session (associate ID), it should use the same mode to send back the `Packet` commands.

A UDP session can be dissociated by sending a `Dissociate` command through a QUIC `unidirectional_stream` by client. The server will remove the UDP session and release the associated UDP socket.

### Heartbeat

When there is any ongoing relaying task, the client should send a `Heartbeat` command through a QUIC `datagram` periodically to keep the QUIC connection alive.

## Error Handling

Note that there is no response for any command. If the server receives a command that is not valid, or encounters any error during the processing (e.g. the target address is unreachable, authentication failure), there is no *standard* way to deal with it. The behavior is implementation-defined. The server may close the QUIC connection, or just ignore the command.

For example, if the server receives a `Connect` command with an unreachable target address, it may close `bidirectional_stream` to indicate the error.
