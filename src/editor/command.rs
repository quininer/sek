use bumpalo::Bump;
use bumpalo::collections::String;
use crate::shell::{ Shell, Action };
use crate::editor::Editor;


pub const EDITOR_COMMANDS: &[(&str, CommandFn)] = &[
    ("q", quit),
    ("quic", quit),
    ("alias-expand", alias_expand),
];

type CommandFn = for<'a> fn(&'a Bump, &'a mut Editor, &'a mut Shell)
    -> anyhow::Result<Action>;

pub fn execute_command(editor: &mut Editor, shell: &mut Shell) -> anyhow::Result<Action> {
    let bump = shell.bump.clone();
    let bump = bump.borrow();

    let mut buf = String::with_capacity_in(8, &bump);
    editor.cmd.read_into(&mut buf);
    editor.cmd.clear();
    let buf = buf.trim_start_matches(':').trim();

    for &(name, editfn) in EDITOR_COMMANDS {
        if buf == name {
            return editfn(&bump, editor, shell);
        }
    }

    Ok(Action::Continue)
}

fn quit(_bump: &Bump, _editor: &mut Editor, _shell: &mut Shell) -> anyhow::Result<Action> {
    Ok(Action::Stop)
}

fn alias_expand(bump: &Bump, editor: &mut Editor, shell: &mut Shell) -> anyhow::Result<Action> {
    let mut buf = String::with_capacity_in(editor.line.len() + 16, bump);
    editor.line.read_into(&mut buf);
    shell.alias.replace(&mut buf);
    editor.line.clear();
    for c in buf.chars() {
        editor.line.push(c);
    }
    editor.line.move_end();

    Ok(Action::Continue)
}
