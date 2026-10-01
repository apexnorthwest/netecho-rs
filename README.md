# netecho-rs
A basic implementation of http responses in rust, intended for load testing routing systems.


In testing, this app can easily handle hundreds of thousands of requests per second on a large, multicore system.
That's because the app doesn't actually do much, but still. :) Hyper is fast.

The app exposes port 8080 and listens on the following endpoints:

- `/` returns a 200 OK always
- `/ok` returns a 200 OK always
- `/add` will add two numbers, provided by the `n1` and `n2` url query parameters. Ex: `/add?n1=123&n2=456`
  - Inputs and outputs are limited to the int64 range
- Any other path will return a 404

Currently the app is only built with http/1.1 support. http/2 support is planned for some future point, as
are additional endpoints for adding extra testing tools. If you have any ideas, we'd love to hear them!

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
