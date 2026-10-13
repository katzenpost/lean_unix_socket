/-
SPDX-FileCopyrightText: Copyright (C) 2026 David Stainton
SPDX-License-Identifier: AGPL-3.0-only
-/

/-! # Unix domain sockets for Lean

Stream sockets to a file path or, after a leading `@`, a Linux abstract name. The socket only
moves bytes; framing is the caller's. The functions are implemented in `rust/src/lib.rs`.

`recv?` blocks its thread, so call it from a dedicated task. `shutdown` wakes it. -/

namespace UnixSocket

private opaque SocketImpl : NonemptyType

/-- A connected stream socket. It closes when no longer referenced. -/
def Socket : Type := SocketImpl.type

instance : Nonempty Socket := SocketImpl.property

namespace Socket

/-- Connect to `path`, or to the abstract name after a leading `@`. -/
@[extern "lus_connect"] opaque connect (path : @& String) : IO Socket

/-- Two sockets connected to each other. -/
@[extern "lus_pair"] opaque pair : IO (Socket × Socket)

/-- Send all of `data`. -/
@[extern "lus_send"] opaque send (s : @& Socket) (data : @& ByteArray) : IO Unit

/-- Wait for up to `max` bytes; none once the peer has shut down its side. -/
@[extern "lus_recv"] opaque recv? (s : @& Socket) (max : USize) : IO (Option ByteArray)

/-- Shut down both directions, waking a blocked `recv?`. -/
@[extern "lus_shutdown"] opaque shutdown (s : @& Socket) : IO Unit

end Socket

/-- For tests: listen on `path`, then accept one connection and send back what it receives until
the peer shuts down. -/
@[extern "lus_echo_once"] opaque echoOnce (path : @& String) : IO Unit

end UnixSocket
