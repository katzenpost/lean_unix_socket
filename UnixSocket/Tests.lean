/-
SPDX-FileCopyrightText: Copyright (C) 2026 David Stainton
SPDX-License-Identifier: AGPL-3.0-only
-/

import UnixSocket

open UnixSocket

def check (name : String) (ok : Bool) : StateT Nat IO Unit := do
  IO.println s!"{if ok then "ok  " else "FAIL"} {name}"
  unless ok do modify (· + 1)

/-- Receive until the peer shuts down. -/
partial def recvAll (s : Socket) (acc : ByteArray := .empty) : IO ByteArray := do
  match ← s.recv? 65536 with
  | some chunk => recvAll s (acc ++ chunk)
  | none => return acc

/-- Receive until `n` bytes have arrived or the peer shuts down. -/
partial def recvN (s : Socket) (n : Nat) (acc : ByteArray := .empty) : IO ByteArray := do
  if acc.size ≥ n then return acc
  match ← s.recv? 65536 with
  | some chunk => recvN s n (acc ++ chunk)
  | none => return acc

def tests : StateT Nat IO Unit := do
  let pid ← IO.Process.getPID

  let (a, b) ← Socket.pair
  let msg := ByteArray.mk <| (Array.range (1 <<< 20)).map (·.toUInt8)
  let sender ← IO.asTask (prio := .dedicated) do a.send msg; a.shutdown
  let got ← recvAll b
  check "a megabyte across a pair" (got == msg && (← IO.wait sender) matches .ok ())

  let (a, _b) ← Socket.pair
  let reader ← IO.asTask (prio := .dedicated) (a.recv? 16)
  IO.sleep 50
  a.shutdown
  check "shutdown wakes a blocked recv" ((← IO.wait reader) matches .ok none)

  let (a, b) ← Socket.pair
  b.shutdown
  check "send after the peer shut down fails" ((← (a.send "x".toUTF8).toBaseIO) matches .error _)

  let name := s!"@lean_unix_socket/test/{pid}"
  echoOnce name
  let s ← Socket.connect name
  s.send "hello".toUTF8
  let echoed ← recvN s 5
  s.shutdown
  check "echo over an abstract name" (echoed == "hello".toUTF8)

  let file := s!".lake/test-{pid}.sock"
  echoOnce file
  let s ← Socket.connect file
  s.send "file".toUTF8
  let echoed ← recvN s 4
  s.shutdown
  check "echo over a file" (echoed == "file".toUTF8)
  IO.FS.removeFile file

  match ← (Socket.connect s!"@lean_unix_socket/nobody/{pid}").toBaseIO with
  | .ok _ => check "no listener on an abstract name" false
  | .error e => check s!"no listener on an abstract name: {e}" true

  let missing := s!".lake/missing-{pid}.sock"
  match ← (Socket.connect missing).toBaseIO with
  | .error e@(.noFileOrDirectory ..) =>
    check s!"no such file: {e}" (((toString e).splitOn missing).length > 1)
  | r => check s!"no such file, got {match r with | .ok _ => "a socket" | .error e => toString e}" false

def main : IO UInt32 := do
  let (_, failed) ← tests.run 0
  IO.println s!"{failed} failed"
  return if failed == 0 then 0 else 1
