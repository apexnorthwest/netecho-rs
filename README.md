# netecho-rs
A basic implementation of http responses in rust, intended for load testing routing systems.


In testing, this app can easily handle hundreds of thousands of requests per second on a large, multicore system.
That's because the app doesn't actually do much, but still. :) Hyper is fast.

The app exposes port 8080 and listens on the following endpoints:

- `/` returns a 200 OK always
- `/ok` returns a 200 OK always
- `/echo` returns a text summary of the whole request the service received
  - Request bodies are limited to 10 MiB by default; larger bodies receive a 413 response
  - Header values with names containing `token`, `secret`, `cookie`, or `authorization` are redacted by default
- `/add?n1=123&n2=456` will add two numbers
  - Inputs and outputs are limited to the int64 range; overflowing sums wrap
- `/bytes?n=1024` will return some random data up to 10MiB (not actually random, the random pool is fixed at process start)
  - `n` is the number of bytes to get
- `/text?n=1024` will return the same data as `/bytes` but base64 encoded (likewise, this is not actually random data)
  - `n` is the number of bytes to get
- `/delay?ms=1000` will return 200 OK after some delay in milliseconds
  - `ms` is how many milliseconds to delay responding
  - Delays above 9000 ms cannot be set safely: request handling times out after 10 seconds
- `/random?n=1024` will return some actually random bytes, up to 10MiB
  - `n` is the number of bytes to get
- `/randtext?n=1024` will return some actually random bytes, up to 10MiB, encoded as base64
  - `n` is the number of bytes to get
- Any other path will return a 404

You can send GET or POST to any endpoint. (Or any method actually, the service doesn't care. You can even make one up.
This is not a standards compliant web server.)

Currently the app is only built with http/1.1 support. http/2 support is planned for some future point, as
are additional endpoints for adding extra testing tools. If you have any ideas, we'd love to hear them!

Request handling and HTTP header reads each have a 10-second timeout. A request handling timeout returns
503; the timeout does not bound response transmission or CPU work that does not yield. Keep-alive
connections can serve multiple requests. On SIGINT or SIGTERM, the server stops accepting connections
and allows up to 10 seconds for active connections to finish.

## Configuration

- `NETECHO_ECHO_MAX_BYTES`: maximum `/echo` request body size in bytes, default `10485760` (10 MiB).
  Set to `0` to accept only empty bodies.
- `NETECHO_ECHO_SECRETS`: set to `true` to echo all header values without redaction; default `false`.

Invalid configuration values cause startup to fail. Redaction only covers the specified header names:
query strings, request bodies, and other headers may contain sensitive data. This server listens on all
interfaces and has no authentication or TLS, so restrict network access to trusted clients.

## Using it
There are two main ways to run it, as a container or as a bare binary.

We publish amd64 and arm64 linux container builds to `ghcr.io/apexnorthwest/netecho-rs:dev` that you can use
rather than building it yourself. The `dev` tag tracks the main branch. It's under 1MB too!

If you want to build it yourself, you can run:
```sh
# Run build from github with cargo. We assume you're using the latest stable rust or the nightly.
# These options will build it optimized for your hardware and locked to the last tested version of all libraries.
RUSTFLAGS="-C target-cpu=native" cargo install --git https://github.com/apexnorthwest/netecho-rs --force --locked

# Run the app. This assumes your cargo is using the default paths.
~/.cargo/bin/netecho-rs
```

To build it as a container:
```sh
# Get the source code
git clone https://github.com/apexnorthwest/netecho-rs.git
# Enter the source directory
cd netecho-rs
# Run the build using docker. Podman and Buildah will also work, our Containerfile is agnostic.
# This will not build an architecture optimized binary, but the performance difference isn't meaningful.
docker build -f Containerfile -t netecho-rs .

# Run the tool. We prefer host networking mode as this app can easily overload docker's overlay network.
docker run --rm --network=host netecho-rs
```

If you want to run this on Kubernetes I presume you know what to do. We'll add a helm chart at some point.
Regardless, it's a dead simple app to run. This is the use case it was designed for, to load test our service mesh.

## Contributing
You may find our [code of conduct](CODE_OF_CONDUCT.md) and [contributor guidelines](CONTRIBUTING.md)
in this repo. We expect everyone to follow the guidelines and policies to ensure a positive,
productive, and enjoyable experience for all involved.

## Fuzz testing

ClusterFuzzLite runs the `socket_http` target with AddressSanitizer on pull requests. The target starts
the same server used by the binary on an ephemeral loopback port and sends each input as raw TCP bytes.
Each input is sent on two connections, one with a write-half close and one without.
The checked-in seed corpus includes all endpoints, chunked input, and pipelined requests. Crashes are
available as workflow artifacts; the workflow does not persist the generated corpus between runs.

Each exchange is cut off after 100 ms so incomplete requests and `/delay` do not stall fuzzing.
Consequently, this target does not test slow-client timing or long delays. ClusterFuzzLite allows
inputs up to 10 MiB plus 64 KiB of HTTP framing.

To run it locally, install a nightly Rust toolchain and `cargo-fuzz`, then:

```sh
mkdir -p fuzz/corpus/socket_http
cp fuzz/seeds/socket_http/* fuzz/corpus/socket_http/
cargo +nightly fuzz run socket_http -- -max_len=10551296
```

To reproduce a crash, run `cargo +nightly fuzz run socket_http path/to/crash-artifact`.
The generated corpus and crash artifacts are ignored by Git.

### Pre-release fuzzing
Maintainers should run a longer session before each release with [fuzz-long.sh](fuzz-long.sh):

```sh
# 4 hours (the default) using half the CPU cores
./fuzz-long.sh
# 12 hours, 8 workers
WORKERS=8 ./fuzz-long.sh 43200
```

The script needs a nightly Rust toolchain and `cargo-fuzz`. It keeps the corpus in `fuzz/corpus/socket_http`
between runs, so later sessions build on earlier ones. It exits non-zero and lists the artifacts if it finds
a crash, timeout, or out-of-memory condition.

Every input opens loopback connections, so many workers leave a large number of sockets in `TIME_WAIT` on
the machine for about a minute (the kernel caps this, typically at 131072). That is harmless on a dedicated
machine, but avoid running the script on a host that is doing other connection-heavy work.

## License
Copyright 2026 Apex Northwest

```
Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
