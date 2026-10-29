## sacrifice-engine (or Tal-Engine) 

sacrifice-engine is a Rust chess engine with a UCI interface that inspired by Mikhail Tal’s attacking style, the greatest attacker in chess history. This engine works on Windows, Linux, and Mac, and here's how to launch it:

- [windows](#windows)
- [linux](#linux)
- [mac](#mac)

## Windows

Compile the engine in release mode:

```bash
cargo build --release
```

Then launch it:

```powershell
.\target\release\Sacrifice-Engine.exe
```

## Linux

Compile the engine:

```bash
cargo build --release
```

Then launch it:

```bash
./target/release/Sacrifice-Engine
```

## Mac

Compile the engine:

```bash
cargo build --release
```

Then launch it:

```bash
./target/release/Sacrifice-Engine
```

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) and Cargo installed.
- A terminal to compile and launch the engine.

## UCI compatibility

Sacrifice-Engine uses the Universal Chess Interface (UCI) protocol and can be used with compatible chess GUIs.

## game :

[game black](https://lichess.org/game/NW5ZGVEZ/black)
game white coming soon...



