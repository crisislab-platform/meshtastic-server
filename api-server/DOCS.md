# CRISiSLab Meshtastic Portal API Server

## Overview

This server is an interface between the MQTT broker (i.e. the gateway nodes in the LoRa mesh) and CRISiSLab's Meshtastic Portal. This document will explain how to use the API, and how to run the server yourself.

## API Endpoints

### `POST /admin/set-mesh-settings`

#### Body

All fields in the body are optional. Only the fields that are specified will be updated on the nodes.

```
{
	broadcast_interval_seconds: unsigned 32 bit int,
	channel_name: string (max 11 chars),
	ping_timeout_seconds: unsigned 32 bit int
}
```

- `broadcast_interval_seconds` is the interval that nodes broadcast their info and telemetry for the server,
- `channel_name` is the name of the channel that all mesh communication will be done on in this tool,
- `ping_timeout_seconds` is how long a node will wait after receiving one ping before it stops listening for pings and sends the data it's gathered to the MQTT broker.

#### Returns

If ok: 200 OK with empty body.
If unexpected error: 500 Internal Server Error with error message in body.
If improperly formatted body: 422 Unprocessable Entity with empty body.

### `POST /admin/set-server-settings`

#### Body

```
{
	signal_data_timeout_seconds: unsigned 32 bit int
}
```

- `signal_data_timeout_seconds` is how long the server will wait for signal data before doing the pathfinding
- all fields are optional

#### Returns

If ok: 200 OK with empty body.
If improperly formatted body: 422 Unprocessable Entity.

### `GET /admin/update-routes`

#### Body

None

#### Returns

```
{
	<start node id>: [
		[
			<first node in path excl. ourselves>
			<second>
			<third>
			...
			<gateway node id>
		],
		...
	],
    ...
}
```

### `WebSocket /info/live`

Each message is a JSON serialised [CrisislabMessage.LiveInfo protobuf](https://github.com/search?q=repo%3Atobyck%2Fcrisislab-meshtastic-protobufs%20crisislab.proto%20LiveData&type=code). Please refer to the linked protobuf definition to see what this contains as it's subject to change. You may also need to refer to protobufs defined by the Meshtastic project, not us. [This website](https://buf.build/meshtastic/protobufs/docs/main:meshtastic) can be helpful for that, otherwise you can search through [our fork of Meshtastic's protobuf repository](https://github.com/tobyck/crisislab-meshtastic-protobufs).

## Running the server

The only dependency that Cargo doesn't handle is the protobuf compiler (`protoc`). Installation will obviously depend on your OS, so refer to the [official documentation](https://protobuf.dev/installation/) for that. If you're on Nix, there's a dev shell you can use instead if you don't want to install it globally. From the `api-server` directory, run:

```
nix develop ..#api
```

Next simply build and run the server with Cargo:

```
RUST_LOG=debug cargo run
```

See the [env_logger documentation](https://docs.rs/env_logger/0.11.8/env_logger/) for the different log levels.
