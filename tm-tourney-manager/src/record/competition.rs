use spacetimedb::{AnonymousViewContext, view};

use crate::record::TmRecord;

#[view(name= competition_record,public)]
pub fn competition_record(ctx: &AnonymousViewContext, /* TODO: match_id arg */) -> Vec<TmRecord> {
    //TODO gather all records of the children of the comp.
    // idk how to do this yet.
    Vec::new()
}
