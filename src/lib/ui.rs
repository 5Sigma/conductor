use crate::{Component, TerminalColor};
use ansi_term::Colour::*;

pub fn system_message(str: String) {
    let l_bracket = Red.bold().paint("-=[");
    let r_bracket = Red.bold().paint("]=-");
    let msg = White.bold().paint(str);

    println!("{} {} {}", l_bracket, msg, r_bracket);
}

pub fn system_error(str: String) {
    let l_bracket = Red.bold().paint("-=[");
    let r_bracket = Red.bold().paint("]=-");
    let msg = Red.bold().paint(str);

    println!("{} {} {}", l_bracket, msg, r_bracket);
}

pub fn component_message(cmp: &Component, msg: String) {
    let name: String = match cmp.color {
        TerminalColor::White => format!("{}", White.bold().paint(&cmp.name)),
        TerminalColor::Blue => format!("{}", Blue.bold().paint(&cmp.name)),
        TerminalColor::Red => format!("{}", Red.bold().paint(&cmp.name)),
        TerminalColor::Green => format!("{}", Green.bold().paint(&cmp.name)),
        TerminalColor::Purple => format!("{}", Purple.bold().paint(&cmp.name)),
        TerminalColor::Yellow => format!("{}", Yellow.bold().paint(&cmp.name)),
    };
    let l_bracket = White.bold().paint("[");
    let r_bracket = White.bold().paint("]");
    println!("{}{}{} {}", l_bracket, name, r_bracket, msg);
}
