use anyhow::Result;

use crate::cli::{InitArgs, InitShell};

const ZSH_WIDGET: &str = include_str!("../shell/zsh_widget.zsh");

pub fn run(args: InitArgs) -> Result<()> {
    match args.shell {
        InitShell::Zsh => {
            print!("{ZSH_WIDGET}");
            Ok(())
        }
    }
}
