# lean_unix_socket

Unix domain stream sockets for Lean 4 programs on Linux, to a file path or an abstract name.
The Rust side uses [lean-sys](https://github.com/katzenpost/lean-sys) for Lean's object model.
A socket only moves bytes; framing is the caller's.

## Use

In your `lakefile`:

```toml
[[require]]
name = "lean_unix_socket"
git = "https://github.com/katzenpost/lean_unix_socket"
rev = "<commit>"
```

```lean
import UnixSocket
open UnixSocket

def main : IO Unit := do
  let s ← Socket.connect "@kpclientd"   -- or a file path
  s.send "hello".toUTF8
  let reader ← IO.asTask (prio := .dedicated) (s.recv? 65536)
  if let some reply ← IO.ofExcept (← IO.wait reader) then
    IO.println s!"{reply.size} bytes"
  s.shutdown
```

`recv?` blocks its thread, so call it from a dedicated task; `shutdown` wakes it. The socket
closes when it is no longer referenced.

Building needs `cargo` on the `PATH`. Lake builds the crate in `rust/` and links it into your
executables.

## Tests

    cd rust && cargo test                              # sockets in Rust
    lake build unix-socket-test && lake exe unix-socket-test   # the same through Lean

## License

AGPL-3.0-only
