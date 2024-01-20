# hoot

no_std, allocation free http 1.1 library.

## API in flux

This library is currently experimental. We're trying to find a good
API that fulfil these criteria:

* Allocation free – To be useful in no_std environments without allocators.
* Ergonomic - As ergonomic as possible while still being allocation free.
* Correct - Encourage (or force) correct HTTP 1.1 usage.

The library has both a client and a server implementation.

License: MIT OR Apache-2.0
