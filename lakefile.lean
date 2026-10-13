import Lake
open Lake DSL System

package lean_unix_socket

@[default_target]
lean_lib UnixSocket

lean_exe «unix-socket-test» where
  root := `UnixSocket.Tests

/-- The Rust crate in `rust/`, built by cargo. -/
extern_lib lean_unix_socket pkg := Job.async do
  proc { cmd := "cargo", args := #["build", "--release", "--quiet"], cwd := pkg.dir / "rust" }
  let lib := pkg.dir / "rust" / "target" / "release" / nameToStaticLib "lean_unix_socket"
  setTrace (← computeTrace lib)
  return lib
