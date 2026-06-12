use rs_matter::error::Error;
use rs_matter::tlv::{TLVTag, TLVWrite};

pub(crate) fn write_nullable<T, W, F>(
    tw: &mut W,
    tag: &TLVTag,
    value: Option<T>,
    write_value: F,
) -> Result<(), Error>
where
    W: TLVWrite + ?Sized,
    F: FnOnce(&mut W, &TLVTag, T) -> Result<(), Error>,
{
    if let Some(value) = value {
        write_value(tw, tag, value)?;
    } else {
        tw.null(tag)?;
    }
    Ok(())
}
