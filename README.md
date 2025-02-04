# Lally

[![Lint](https://github.com/lovelindhoni/lally/actions/workflows/lint.yml/badge.svg)](https://github.com/lovelindhoni/lally/actions/workflows/lint.yml)

Lally is a distributed in-memory key-value database, Rust and Tokio, loosely inspired by Riak and Voldermort.
It prioritizes high availability, adhering to the AP (Availability and Partition Tolerance) aspects of the CAP theorem.
Lally is being designed for high concurrency, extendability via hooks, and features persistent append-only file logging for reliable crash recovery.

> [!NOTE]
> Lally is currently in the early stages of development and is not yet ready for serious production use.

## Demo

A Lally cluster consisting of three globally distributed EC2 instances.

https://github.com/user-attachments/assets/6ec0d3df-a9d7-4a50-846b-08045bf3ebb5

## Features

- **High Performance**: A highly concurrent, thread-safe, in-memory key-value database optimized to utilize all available CPU cores responsibly.
- **Extensible Design**: Hooks-based architecture allows custom behaviors to be invoked during any key-value operation.
- **Crash Recovery**: Supports append-only file (AOF) logging for robust crash recovery, easy backup, replay, and data restoration.
- **Cluster Joining**: Enables seamless IP-based cluster joining through a seed node for distributed operation.
- **Data Replication**: Achieves data replication across cluster nodes using lightweight and efficient Protocol Buffers through gossipping
- **Connection Pooling**: Reduces overhead by pooling gRPC connections, avoiding repeated connection establishment for inter-node communication.
- **Quorum Flexibility**: Configurable read and write quorum settings to match the size and needs of the cluster.
- **Flexible Configuration**: YAML-based configuration that can be overridden using command-line arguments for customization.
- **Graceful Shutdown**: Ensures proper cluster exit and resource cleanup during shutdown, maintaining cluster stability.
- **Comprehensive Logging**: Includes extensive logging capabilities for debugging and tracing operations effectively.
- **User-friendly HTTP API**: Offers an intuitive and straightforward HTTP API for managing the key-value store.
- **Read Repair Mechanism**: Automatically resolves stale or outdated data during read operations to maintain consistency.
- **Safe and Reliable (👀)**: Free from unsafe code blocks

## Configuration

You can configure Lally by passing the following CLI arguments:

- `--config`: Path to the configuration file (e.g., lally.yml).
- `--fresh`: Wipes previous WAL (Write-Ahead Log) data and starts fresh. (This will clear the existing AOF file, skipping replay of previous operations)
- `--replay-log`: Path to a custom AOF file for replay. The contents of this file will replace and replay the default AOF file used by Lally.
- `--seed-node`: IPv4 address with the port of the seed node. Required for joining a cluster via the seed node.
- `--http-port`: Custom port for the HTTP server (default: 3000).
- `--grpc-port`: Custom port for the gRPC server (default: 50071).
- `--read-quorum`: Specifies the number of nodes required for a successful read operation (default: 1).
- `--write-quorum`: Specifies the number of nodes required for a successful write operation (default: 1).
- `--help`: Displays detailed usage information.

Lally also supports configuration through a YAML file for greater flexibility and ease of use.

By default, Lally looks for a lally.yml file in the current directory where the binary is executed. Alternatively, you can specify a custom configuration file using the `--config` CLI argument, like this:

```bash
lally --config /path/to/lally.yml
```

### Example lally.yml with Default Values

```yaml
fresh: false # Start fresh, wiping the previous AOF log (default: false)
replay_log: None # Path to a custom AOF log file for replay
seed_node: None # IPv4 address and port of the seed node (if joining a cluster)
grpc_port: 50071 # Port for the gRPC server
http_port: 3000 # Port for the HTTP server
read_quorum: 1 # Number of nodes required for a successful read operation
write_quorum: 1 # Number of nodes required for a successful write operation
```

### Priority of Configuration

Command-line arguments (`--config`, `--http-port`, etc.) always override the values specified in the YAML file.
This allows for quick runtime adjustments without the need to modify the configuration file.
For example, even if http_port is set to 3000 in the YAML file, specifying `--http-port 8080` via CLI will use 8080 for that session.

## HTTP Endpoints

Lally provides a simple HTTP API for interacting with the distributed key-value store.

### POST /get

Fetches the latest value associated with the given key. If the value is stale, it triggers a read repair to ensure eventual consistency.

#### Request

```jsonc
{
  "key": "example_key",
}
```

#### Expected Response

```jsonc
{
  "status": "success | partial | error",
  "key": "example_key",
  "value": "example_value | null",
  "timestamp": "RFC3339 timestamp | null",
  "quorum": {
    "required": 2, // Number of nodes needed for quorum
    "achieved": 2, // Number of nodes that responded
  },
  "message": "Human-readable explanation",
}
```

### POST /add

Adds a key-value pair to the cluster. If the key already exists, its value will be updated.

#### Request

```jsonc
{
  "key": "example_key",
  "value": "example_value",
}
```

#### Expected Response

```jsonc
{
  "status": "success | partial",
  "key": "example_key",
  "value": "example_value",
  "timestamp": "RFC3339 timestamp",
  "quorum": {
    "required": 2, // Number of nodes needed for quorum
    "achieved": 2, // Number of nodes that responded
  },
  "message": "Operation completed successfully.",
}
```

### DELETE /remove

Fetches the latest value associated with the given key. If the value is stale, it triggers a read repair to ensure consistency.

#### Request

```jsonc
{
  "key": "example_key",
}
```

#### Expected Response

```jsonc
{
  "status": "success | partial | error",
  "key": "example_key",
  "value": "example_value | null", // Removed value (if applicable)
  "timestamp": "RFC3339 timestamp | null", // Timestamp of the removal operation
  "quorum": {
    "required": 2, // Number of nodes needed for quorum
    "achieved": 1, // Number of nodes that responded
  },
  "message": "Key successfully removed.", // Human-readable explanation
}
```

### GET /nodes

Retrieves a list of all network addresses of nodes in the cluster, excluding the node handling the request.

#### Expected Response

```jsonc
{
  "status": "success | error",
  "nodes": ["192.168.1.1:50071", "192.168.1.2:50071"],
}
```

### Key Notes

**Quorum State**: The status field in responses indicates the quorum state:

- **success**: The quorum is fully satisfied.
- **partial**: Quorum partially satisfied; some nodes failed to respond.
- **error**: Error :|

**Timestamp Format**: All timestamps are in RFC3339 format for standardization.

**Quorum Details**: The quorum field provides insight into the required and achieved votes during the operation, helping debug cluster consistency

## Development

To get started with developing or running Lally, ensure you have the required dependencies and follow the setup instructions below.

### Dependencies

A suitable Rust toolchain.
Protocol Buffers Compiler (protoc) version 3 or higher
Protocol Buffers development headers.

### Setup instructions

1. Clone the repository

```bash
git clone https://github.com/lovelindhoni/lally.git
cd lally
```

2. Configure network settings:

Allow inbound and outbound TCP traffic on the ports used by Lally.

Default HTTP port: 3000
Default gRPC port: 50071
Ensure your firewall settings permit these ports or any custom ports you’ve configured.

3. Debug mode

```bash
cargo run
```

## Production Build

Build the release binary:

```bash
cargo build --release
```

Locate the compiled binary in target/release/lally

### Example exectution

```bash
./lally --config lally.yml --http_port 4001
```

This binary is optimized for performance and ready for deployment.

## Roadmap (tentative)

- Kubernetes compatibility for deployment and scalability
- Testing suite and performance benchmarking 👀
- Periodic health checks to monitor the status of transparent channels between nodes in the pool
- Background reconciliation (anti-entropy) process for automatic key value repairs, to fulfill eventual consistency
- CI pipeline for building, testing, and releasing Lally across all platforms
- Enhanced, structured trace logging for better observability
- Web-based UI for intuitive interaction with Lally
- Support for additional data structures—currently, only string-to-string pairs are supported, but we aim to streamline support for other data structures via more efficient deserialization

## Contribution

Please submit pull requests if you had to, after your code is linted and formatted using Clippy and Rustfmt.
You can find the required checks in the ci.yml file, which lists all the validation processes

## LICENSE

**MIT**
