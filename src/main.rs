use chrono::prelude::*;
use clap::Parser;
use imap::error::Result;
use imap::Session;
use itertools::Itertools;
use std::io::{Read, Write};
use std::ops::RangeInclusive;

/// Simple program to greet a person
#[derive(clap::Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Host name to connect to.
    #[clap(short, long)]
    host: String,

    /// Host port to connect to.
    #[clap(short, long, default_value = "993")]
    port: u16,

    /// Username.
    #[clap(short, long)]
    username: String,

    /// Before date.
    #[clap(long, value_parser(parse_date))]
    before: Date<Local>,

    #[clap(long, short = 'b', default_value = "INBOX")]
    mailbox: String,

    /// Host port to connect to.
    #[clap(short = 'n', long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let password = rpassword::prompt_password("Password: ").unwrap();
    let tls = native_tls::TlsConnector::builder().build()?;
    let client = imap::connect((args.host.as_str(), args.port), &args.host, &tls)?;
    let mut session = client.login(&args.username, password).map_err(|e| e.0)?;
    cleanup_emails(&mut session, &args.mailbox, args.before, args.dry_run)
}

fn parse_date(s: &str) -> chrono::ParseResult<Date<Local>> {
    Ok(Local
        .from_local_date(&NaiveDate::parse_from_str(s, "%Y-%m-%d")?)
        .unwrap())
}

fn cleanup_emails<S: Read + Write, Tz: TimeZone>(
    session: &mut Session<S>,
    mailbox: &str,
    before: Date<Tz>,
    dry_run: bool,
) -> Result<()> {
    let _ = session.select(mailbox)?;
    let mut uids = session
        .search(
            before
                .naive_utc()
                .format("BEFORE %-e-%b-%Y NOT FLAGGED")
                .to_string(),
        )?
        .into_iter()
        .collect::<Vec<_>>();
    uids.sort();
    if dry_run {
        for range in ranges(&uids) {
            let fetch = session.fetch(
                format!("{}:{}", range.start(), range.end()),
                "(INTERNALDATE FLAGS)",
            )?;
            for message in &fetch {
                let internal_date = message.internal_date().unwrap();
                println!("{} {:?}", internal_date, message.flags());
            }
        }
        println!("{} not deleted (dry run).", uids.len());
    } else {
        for range in ranges(&uids) {
            session.store(
                format!("{}:{}", range.start(), range.end()),
                r"+FLAGS.SILENT (\Deleted)",
            )?;
        }
        session.expunge()?;
        println!("{} deleted.", uids.len());
    }
    Ok(())
}

fn ranges<'a>(uids: impl IntoIterator<Item = &'a u32> + 'a) -> Vec<RangeInclusive<u32>> {
    let uids = uids.into_iter();
    let mut previous = None;
    let mut start = previous;

    (&uids.group_by(|x| {
        match previous {
            Some(previous) if **x != previous + 1 => start = Some(**x),
            None => start = Some(**x),
            _ => {}
        }
        previous = Some(**x);
        start
    }))
        .into_iter()
        .map(|(start, group)| start.unwrap()..=(*group.last().unwrap()))
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn range() {
        assert_eq!(ranges(&[1, 2, 3]), &[1..=3]);
        assert_eq!(ranges(&[1, 2, 3, 7, 8, 9]), &[1..=3, 7..=9]);
        assert_eq!(ranges(&[1, 2, 3, 5, 7, 8, 9]), &[1..=3, 5..=5, 7..=9]);

        assert_eq!(ranges(&[]), &[]);
        assert_eq!(ranges(&[1, 3]), &[1..=1, 3..=3]);
    }
}
