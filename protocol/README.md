# tuic-protocol

TUIC protocol is used to communicate between the TUIC client and the TUIC server.

## Overview

TUIC protocol is a stateful protocol. It is designed to be simple yet efficient. The current version is `0x05`.

## Command

Relay tasks are negotiated with `Command`s.
All fields are in Big Endian unless otherwise noted.

```plain
+-----+------+----------+
| VER | TYPE |   OPT    |
+-----+------+----------+
|  1  |  1   | Variable |
+-----+------+----------+
```

where:

- `VER` - protocol version
- `TYPE` - command type
- `OPT` - command type specific data

### Command Types

There are six types of commands:

- `0x00` - `Authenticate` - used to authenticate the client
- `0x01` - `Connect` - used to request a client-to-server TCP relay
- `0x02` - `Packet` - used to forward a UDP packet
- `0x03` - `Dissociate` - used to stop a UDP relay session
- `0x04` - `Heartbeat` - used to keep a QUIC connection alive
- `0x05` - `LongPacket` - used to forward a fragmented UDP packet
- `0xff` - `Response` - used to respond to a `Command` (currently only used for replying `Connect`)

### Command Type Specific Data

#### `Authenticate`

```plain
+-----+
| TKN |
+-----+
| 32  |
+-----+
```

where:

- `TKN` - authentication token, hashed with [BLAKE3](https://github.com/BLAKE3-team/BLAKE3)

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
+----------+-----+----------+
| ASSOC_ID | LEN |   ADDR   |
+----------+-----+----------+
|    4     |  2  | Variable |
+----------+-----+----------+
```

where:

- `ASSOC_ID` - UDP relay session ID. See [UDP relaying](#udp-relaying)
- `LEN` - length of the UDP packet
- `ADDR` - target (command from TUIC client) or source (command from TUIC server) address. See [Address](#address)

#### `LongPacket`
```plain
+----------+-----+-------+---------+----------+----------+
| ASSOC_ID | LEN | LP_ID | FRAG_ID | FRAG_CNT |  ADDR?   |
+----------+-----+-------+---------+----------+----------+
|    4     |  2  |   4   |    1    |    1     | Variable |
+----------+-----+-------+---------+----------+----------+
```

where:

- `ASSOC_ID` - UDP relay session ID. See [UDP relaying](#udp-relaying)
- `LEN` - length of the UDP packet
- `LP_ID` - ID of the long packet. Must be unique within a UDP session
- `FRAG_ID` - index of the packet fragment, 0-based
- `FRAG_CNT` - total number of fragments
- `ADDR?` - target (command from TUIC client) or source (command from TUIC server) address. See [Address](#address). 
Only present when `FRAG_ID` is 0. In other cases the payload follows `FRAG_CNT` directly.

Implementor should maintain a packet reassembly buffer for each `(ASSOC_ID, LP_ID)` pair. The reassembled packet 
should be available only after all fragments have arrived. 

If multiple fragments with the same `FRAG_ID` are received for one long packet,
only the first one should be considered, while the rest must be dropped.

If multiple fragments with different `FRAG_CNT` are received for one long packet,
only that of the first one should be used, while the rest must be dropped.

If a fragment with `FRAG_ID` >= `FRAG_CNT` is received, it must be dropped.

#### `Dissociate`

```plain
+----------+
| ASSOC_ID |
+----------+
|    4     |
+----------+
```

#### `Heartbeat`

```plain
+-+
| |
+-+
| |
+-+
```

#### `Response`

```plain
+-----+
| REP |
+-----+
|  1  |
+-----+
```

where:

- `REP` - reply code, which can be:

- `0x00` - SUCCEEDED
- `0xff` - FAILED

### Address

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

- `0x00` - fully-qualified domain name(the first byte indicates the length of the domain name)
- `0x01` - IPv4 address
- `0x02` - IPv6 address

## Procedures

TUIC protocol relies heavily on the multiplex-able trusted channel provided by QUIC. The protocol itself does not provide any security.

### Authentication

Once the QUIC connection is established between the server and the client, the client must immediately open a unidirectional stream and send an `Authenticate` command.

If the authentication token is unmatched, or the server does not receive an authentication request from the client within the set time, the server will close the QUIC connection with specific error code and reason. See [Error Handling](#error-handling) for more details.

Note that the server will not reply to the `Authenticate` command. The client should close the stream immediately after successfully sending the command. The client can start sending other data without waiting for the `Authenticate` command to be sent.

The server will accept other streams carrying relay task requests before the authentication is completed, but it will stop after the Command Header is read, and will not do actual processing until the authentication is completed.

### TCP Relaying

`Connect` is used to request a client-to-server TCP relay.

To establish a TCP connection with the target address via the relay server, the client needs to open a bidirectional stream and send a `Connect` command. After the server receives the request, it will try to establish a TCP connection to the target address. Depending on success, the server replies with a `Response` command via the same bidirectional stream.

If the attempt to connect to the target address fails, the server must close the bidirectional stream as soon as the `Response` transmission is complete.

If the connection to the target is successful, the server will synchronize the data in the bidirectional stream with the TCP stream between the server and the target address until one of the streams is disconnected.

### UDP Relaying

TUIC achieves 0-RTT FullCone UDP forwarding by synchronizing UDP session ID between the client and the server.

The server should create a UDP session table for each QUIC connection, mapping every associate ID to a UDP socket.

The associate ID is a 32-bit unsigned integer randomly generated by the client, which is placed in the `Packet` command and appended to the UDP packet data to be sent. When the client wants to send UDP packets using the same UDP socket of the server, the attached associate ID should be the same.

When the server receives the `Packet` command, it should check whether the attached associate ID is already associated with a UDP socket. If not, the server should allocate a UDP socket for the associate ID. The server will use this UDP socket to send UDP packets requested by the client, and accepting UDP packets from any destination at the same time, appends the `Packet` command then sends back to the client.

When the client wants to relay a UDP packet, it should send the UDP packet with the `Packet` command attached from:

- Unidirectional stream (UDP relay mode `quic`)
- Datagram (UDP relay mode `native`)

When the server receives the first `Packet` command, it will consider that the client is using corresponded UDP relay mode. When the UDP socket associated receives a UDP packet, the server should send the packet back to the client in the same way.

When a client wants to stop associating a UDP socket, it should notify the server by sending a `Dissociate` command using a unidirectional stream. The server will remove the associate ID and release the UDP socket from the UDP session table.

When the QUIC connection is disconnected, the server will release all UDP sockets in the connection's UDP session table and delete all sessions.

### Heartbeat

Even if there is an unclosed stream between the server and the client, the QUIC connection will still timeout after a period of idle time. This affects the timeout behavior for tasks without persistent data transfer (such as SSH connections).

To solve this problem, when there is an active relay task (TCP relaying or UDP session), the client should send a `Heartbeat` command to the server every few seconds to keep the connection alive.

### Error Handling

When the server detects the following errors, it should close the QUIC connection immediately with the corresponding error code:

- Protocol Error - `0xfffffff0` - TUIC protocol version mismatch, or the server cannot parse the header
- Authentication Failed - `0xfffffff1` - Authentication token mismatch
- Authentication Timeout - `0xfffffff2` - Authentication timeout
- Bad Command - `0xfffffff3` - Command received from wrong stream / datagram
