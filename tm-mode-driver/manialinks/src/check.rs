//! What a manialink of a competition has to be before a server shows it: small enough,
//! well formed and reading the netvars of the driver with the types they have.

use tm_server_types::method::{DRIVER_NETVARS, DRIVER_PLAYER_NETVARS};

/// The largest manialink a competition can save, in bytes. The defaults are below
/// 30 KB. What a layer of the game can carry is not documented, so this stays well
/// below the limit of an XML-RPC call.
pub const MAX_SIZE: usize = 256 * 1024;

/// Checks a manialink, the error tells what is wrong.
pub fn check(xml: &str) -> Result<(), String> {
    if xml.len() > MAX_SIZE {
        return Err(format!(
            "The manialink has {} KB, at most {} KB fit.",
            xml.len() / 1024,
            MAX_SIZE / 1024
        ));
    }
    well_formed(xml)?;
    for (line, declaration) in xml.lines().enumerate() {
        if let Some(read) = declaration.trim().strip_prefix("declare netread ") {
            netvar(read).map_err(|error| format!("Line {}: {error}", line + 1))?;
        }
    }
    Ok(())
}

/// A netvar of the driver has to be one the modescript writes, with its type and owner.
fn netvar(read: &str) -> Result<(), String> {
    let read = read.trim_end_matches(';').trim();
    let Some((declaration, owner)) = read.split_once(" for ") else {
        return Ok(());
    };
    let Some((kind, name)) = declaration.trim().rsplit_once(' ') else {
        return Ok(());
    };
    if !name.starts_with("Net_Driver_") {
        return Ok(());
    }
    let netvars: &[(&str, &str)] = match owner.trim() {
        "Teams[0]" => &DRIVER_NETVARS,
        "UI" => &DRIVER_PLAYER_NETVARS,
        owner => return Err(format!("{name} is no netvar of the driver for {owner}")),
    };
    match netvars.iter().find(|(netvar, _)| *netvar == name) {
        Some((_, expected)) if *expected == kind => Ok(()),
        Some((_, expected)) => Err(format!("{name} is {expected}, not {kind}")),
        None => Err(format!("{name} is no netvar of the driver")),
    }
}

/// Elements are closed in order and one `manialink` holds everything. Comments, which
/// hold the scripts, and quoted attributes may contain anything.
fn well_formed(xml: &str) -> Result<(), String> {
    let mut open: Vec<&str> = Vec::new();
    let mut roots = 0;
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        let text = &rest[..start];
        if open.is_empty() && !text.trim().is_empty() {
            return Err("There is text outside of the manialink.".into());
        }
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix("<!--") {
            let end = after.find("-->").ok_or("A comment is not closed.")?;
            rest = &after[end + 3..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after.find("]]>").ok_or("A CDATA section is not closed.")?;
            rest = &after[end + 3..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("<?") {
            let end = after.find("?>").ok_or("A declaration is not closed.")?;
            rest = &after[end + 2..];
            continue;
        }
        let end = tag_end(rest).ok_or("A tag is not closed.")?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            match open.pop() {
                Some(opened) if opened == name => {}
                Some(opened) => return Err(format!("</{name}> closes <{opened}>.")),
                None => return Err(format!("</{name}> closes nothing.")),
            }
            continue;
        }
        let name = tag
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default();
        if name.is_empty() {
            return Err("A tag has no name.".into());
        }
        if open.is_empty() {
            roots += 1;
            if name != "manialink" || roots > 1 {
                return Err("Everything has to be in one <manialink>.".into());
            }
        }
        if !tag.ends_with('/') {
            open.push(name);
        }
    }
    if !rest.trim().is_empty() && open.is_empty() {
        return Err("There is text outside of the manialink.".into());
    }
    if let Some(opened) = open.last() {
        return Err(format!("<{opened}> is not closed."));
    }
    if roots == 0 {
        return Err("There is no <manialink>.".into());
    }
    Ok(())
}

/// Where a tag ends, `>` inside quotes does not.
fn tag_end(tag: &str) -> Option<usize> {
    let mut quote = None;
    for (index, c) in tag.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_pass() {
        for default in crate::DEFAULTS {
            assert_eq!(check(default.manialink), Ok(()), "{}", default.name);
        }
    }

    #[test]
    fn manialinks_have_to_be_well_formed() {
        let script = r#"<manialink version="3"><frame><label text="a > b"/></frame><script><!-- if (a < b) {} --></script></manialink>"#;
        assert_eq!(check(script), Ok(()));
        assert_eq!(
            check("<manialink><frame></manialink>").unwrap_err(),
            "</manialink> closes <frame>."
        );
        assert_eq!(
            check("<manialink><frame>").unwrap_err(),
            "<frame> is not closed."
        );
        assert_eq!(
            check("<frame/>").unwrap_err(),
            "Everything has to be in one <manialink>."
        );
        assert_eq!(
            check("<manialink/><manialink/>").unwrap_err(),
            "Everything has to be in one <manialink>."
        );
        assert_eq!(
            check("<manialink/> trailing").unwrap_err(),
            "There is text outside of the manialink."
        );
        assert_eq!(
            check("<manialink><!-- open").unwrap_err(),
            "A comment is not closed."
        );
        assert_eq!(check("").unwrap_err(), "There is no <manialink>.");
    }

    #[test]
    fn netvars_of_the_driver_need_their_type_and_owner() {
        let reading =
            |line: &str| format!("<manialink><script><!--\n{line}\n--></script></manialink>");
        assert_eq!(
            check(&reading(
                "declare netread Integer[] Net_Driver_StandingsScores for Teams[0];"
            )),
            Ok(())
        );
        assert_eq!(
            check(&reading(
                "declare netread Integer Net_Driver_MyUser for UI;"
            )),
            Ok(())
        );
        // Netvars of others are no business of the driver.
        assert_eq!(
            check(&reading("declare netread Integer Net_Other for UI;")),
            Ok(())
        );
        assert_eq!(
            check(&reading(
                "declare netread Integer Net_Driver_StandingsScores for Teams[0];"
            ))
            .unwrap_err(),
            "Line 2: Net_Driver_StandingsScores is Integer[], not Integer"
        );
        assert_eq!(
            check(&reading(
                "declare netread Integer Net_Driver_Nothing for Teams[0];"
            ))
            .unwrap_err(),
            "Line 2: Net_Driver_Nothing is no netvar of the driver"
        );
        assert_eq!(
            check(&reading(
                "declare netread Integer Net_Driver_MyUser for Teams[0];"
            ))
            .unwrap_err(),
            "Line 2: Net_Driver_MyUser is no netvar of the driver"
        );
    }

    #[test]
    fn manialinks_too_large_are_refused() {
        let large = format!("<manialink>{}</manialink>", " ".repeat(MAX_SIZE));
        assert!(check(&large).is_err_and(|e| e.contains("at most 256 KB")));
    }
}
