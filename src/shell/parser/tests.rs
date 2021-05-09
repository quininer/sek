use bumpalo::Bump;
use crate::shell::parser::parse_in;
use crate::shell::parser::type_::*;


#[test]
fn test_parse_command() -> anyhow::Result<()> {
    let mut bump = Bump::new();

    // simple
    bump.reset();
    {
        let input = "exe 123";
        let cmd = parse_in(&bump, input).unwrap();

        assert_eq!(cmd.args.len(), 2);

        assert_eq!(cmd.args[0].0.len(), 1);
        if let ArgSlice::Str(Literal(span)) = &cmd.args[0].0[0] {
            assert_eq!(&input[span.clone()], "exe");
        } else {
            panic!()
        }

        assert_eq!(cmd.args[1].0.len(), 1);
        if let ArgSlice::Str(Literal(span)) = &cmd.args[1].0[0] {
            assert_eq!(&input[span.clone()], "123");
        } else {
            panic!()
        }
    }

    // subshell
    bump.reset();
    {
        let input = r#"exe $(exe2 hello world) "$(exe3)" subshell"#;
        let cmd = parse_in(&bump, input).unwrap();

        eprintln!("{:?}", &cmd);

        assert_eq!(cmd.args.len(), 4);

        assert_eq!(cmd.args[0].0.len(), 1);
        if let ArgSlice::Str(Literal(span)) = &cmd.args[0].0[0] {
            assert_eq!(&input[span.clone()], "exe");
        } else {
            panic!()
        }

        // ...
    }

    Ok(())
}
