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

| Field | Description |
| ----- | ----------- |
| `broadcast_interval_seconds` | The interval at which nodes should broadcast live telemetry to the server |
| `channel_name` | The name of the channel that all mesh communication will be done on with this tool. Maximum 11 characters. |
| `ping_timeout_seconds` | How long a node will wait after receiving one ping before it stops listening for pings and sends the data it's gathered. |

#### Returns

| Situation | Status | Response Body |
| --------- | :----: | :-----------: |
| Ok        | 200 OK | Empty body |
| Improperly formatted body | 422 Unprocessable Entity | Empty body |
| Unexpected error | 500 Internal Server Error | Error message in `error` field of JSON object |

### `GET /get-mesh-settings`

#### Body

None

#### Returns

| Situation | Status | Response Body |
| --------- | :----: | :-----------: |
| Ok        | 200 OK | JSON object like the body of `/admin/set-mesh-settings`. See above. |
| Timeout waiting for mesh | 504 Gateway Timeout | Error message in `error` field of JSON object |
| Unexpected error | 500 Internal Server Error | // |

If ok: 200 OK with a 
If unexpected error: 500 Internal Server Error with error message in body.


### `POST /admin/set-server-settings`

#### Body

```
{
    get_settings_timeout_seconds: unsigned 64 bit int,
    signal_data_timeout_seconds: unsigned 64 bit int,
    route_cost_weight: 32 bit float,
    route_hops_weight: 32 bit float
}
```

All fields are optional.

| Field | Description |
| ----- | ----------- |
| `get_settings_timeout_seconds` | How long the server will wait for a response from the first gateway for a mesh settings response |
| `signal_data_timeout_seconds` | How long the server will wait for signal data from the mesh before doing the pathfinding |
| `route_cost_weight` | The pathfinding algorithm prioritises routes based not only on their distances (i.e. sum of costs), but also the number of hops. This setting affects how much the algorithm prefers routes with a lower cost. |
| `route_hops_weight` | Ditto but for how much it prefers routes with fewer hops. |

#### Returns

| Situation | Status | Response Body |
| --------- | :----: | :-----------: |
| Ok        | 200 OK | Empty body |
| Improperly formatted body | 422 Unprocessable Entity | Empty body |
| Unexpected error | 500 Internal Server Error | Error message in `error` field of JSON object |

### `GET /admin/update-routes`

#### Body

None

#### Returns

```
{
    <start node id>: [
        <best next hop>,
        <next best next hop>,
        ...,
        <worst next hop>
    ],
    ...
}
```

### `WebSocket /info/live`

A live stream of telemetry from every node in the mesh. Each node will broadcast a message at the interval configured using `/admin/set-mesh-settings`. Each message is a JSON serialised [CrisislabMessage.LiveInfo protobuf](https://github.com/search?q=repo%3Atobyck%2Fcrisislab-meshtastic-protobufs%20crisislab.proto%20LiveData&type=code). Please refer to the linked protobuf definition to see what this contains as it's subject to change. You may also need to refer to protobufs defined by the Meshtastic project, not us. [This website](https://buf.build/meshtastic/protobufs/docs/main:meshtastic) can be helpful for that, otherwise you can search through [our fork of Meshtastic's protobuf repository](https://github.com/tobyck/crisislab-meshtastic-protobufs).

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
