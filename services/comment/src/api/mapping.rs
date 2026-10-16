use comment_contracts::CommentDto;

use crate::infrastructure::comments::CommentRow;

pub(super) fn to_dto(row: &CommentRow) -> CommentDto {
    CommentDto {
        id: row.id,
        target_type: row.target(),
        target_id: row.target_id,
        author_id: row.author_id,
        author_name: row.author_name.clone(),
        body: row.body.clone(),
        created_at: row.created_at.to_rfc3339(),
        edited_at: row.edited_at.map(|t| t.to_rfc3339()),
    }
}
