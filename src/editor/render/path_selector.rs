use bumpalo::Bump;
use crate::editor::Editor;
use crate::shell::Shell;


pub fn render(
    bump: &Bump,
    editor: &mut Editor,
    shell: &mut Shell,
) -> anyhow::Result<()> {
    let mut term = shell.term.lock();

    todo!()
}
