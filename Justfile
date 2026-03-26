default:
    @just --list

reinstall:
    cargo install --path . --root "$HOME/.local" --force
