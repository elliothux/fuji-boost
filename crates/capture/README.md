# Fuji capture

Standalone acquisition CLI using the checked-in `references/fujimod/crates/usb`
SDK. The user permits low-risk analysis operations after assessing brick risk.
Ordinary read-only identity queries may check the current body; debug/service
entry and memory writes still require their own evidence and risk assessment.
The commands retained in source describe code paths, not automatic qualification
of service access or writes on a connected body. Current
firmware research uses the X-T4 2.12 source under
`research/firmware/xt4-212-current`; no historical body response qualifies it.

The crate is a separate Cargo workspace and does not build the XP4 emulator or
Wasm. It has no memory-write, debug-enable, reset, mode-switch or flash command.

## Offline verification

From the repository root:

```sh
export CARGO_TARGET_DIR="$PWD/.cache/capture-target"
cargo test --locked --manifest-path crates/capture/Cargo.toml
cargo clippy --locked --manifest-path crates/capture/Cargo.toml --all-targets -- -D warnings
```

These tests use synthetic containers, scripted transports and temporary files.
They never discover or open USB devices and do not depend on a saved camera
capture. Test directories are created under
`research/firmware/xt4-212-current/capture-test-output` and removed after use.
Current validation logs belong beside that directory, not in a historical dump.

## Retained protocol boundaries

The following is a source description, not a camera-operation procedure:

- `list` discovers local Fujifilm VID `04cb` descriptors and reports bulk interface
  selections. It does not claim/configure an interface or send service requests.
- `ptp-info` accepts only an explicitly selected local `04cb:02e6` device and
  sends `GetDeviceInfo` (`1001`, transaction0) without opening a session. It
  requires a single class-6 bulk interface in the active configuration and refuses
  a configuration/alternate change, driver detach, reset or vendor operation.
- `ptp-firmware` uses the same PID/serial restriction. Its fixed sequence is
  `OpenSession` (`1002`, session1/TID0), `GetDevicePropValue` (`1015`, D153/TID1),
  then `CloseSession` (`1003`, TID2) when completion is certain. D153's label is
  sourced from libgphoto2; its datatype/value are not interpreted as body firmware
  identity. The descriptor is not queried. No property is changed.
- `service-info` and `ram` require an already-present local `04cb:ff80` device,
  explicit serial and exactly one bulk interface selection. They never perform a
  mode transition. Their presence in source does not establish safe service
  entry/exit or qualify a debug write as low risk.

Only a confirmed successful open owns a PTP session. A complete error reply may
close that owned session; `SessionAlreadyOpen (201e)` does not close somebody
else's session. Timeout, short transfer or malformed response fences further
commands, including close. There is no arbitrary PTP command, retry, continuation
or session-resume option. Container framing accepts a zero-byte data phase and
consumes its terminal response; application validation rejects empty identity or
firmware datasets. A framed PTP error is a business failure, not identity proof.

Raw transfers and transfer status are saved before container decoding. PTP
reception is bounded to 1 MiB, three seconds per transfer and ten seconds/64
transfers per transaction. Reports retain query and close errors separately and
do not establish firmware identity from an untyped value. Implicit remote USB
proxies are refused through `FUJIMOD_USB_PROXY_URL`.

The RAM range fence remains `[0,0x20000000)`: it does not prove that every address
is RAM, establish capacity, or qualify a firmware's actual read permission.
Ranges and read limits must be explicit; chunks default to256 and must fit the
selected limit and the 65536-byte host cap. The SDK also enforces advertised
packet/reply bounds. No automatic sizing, MMIO, Flash probing, retry or resume is
implemented. Transfer uncertainty keeps the session fenced as
`CompletionUnknown`; a ready session receives one explicit close.

Outputs use exclusive creation and mode0600 on Unix. RAM chunks are synced and
journaled with SHA-256 before final no-overwrite hard-link publication. Failure
preserves the accepted incomplete prefix; a complete manifest is required to
claim completion. A live range would not be an atomic snapshot. Existing
parser/range bounds, session ownership, malformed/empty data rejection and
no-overwrite/partial-preservation tests remain applicable offline.
