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
