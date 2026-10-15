use client_contracts::{ClientDto, ContactDto, PositionDto};

use crate::infrastructure::{clients::ClientRow, contacts::ContactRow, positions::PositionRow};

pub(super) fn client_to_dto(row: &ClientRow) -> ClientDto {
    ClientDto {
        id: row.id,
        name: row.name.clone(),
        note: row.note.clone(),
        archived: row.archived,
        contacts_count: row.contacts_count,
        created_at: row.created_at.to_rfc3339(),
    }
}

pub(super) fn position_to_dto(row: &PositionRow) -> PositionDto {
    PositionDto {
        id: row.id,
        name: row.name.clone(),
    }
}

pub(super) fn contact_to_dto(row: &ContactRow) -> ContactDto {
    ContactDto {
        id: row.id,
        client_id: row.client_id,
        full_name: row.full_name.clone(),
        position: row
            .position_id
            .zip(row.position_name.clone())
            .map(|(id, name)| PositionDto { id, name }),
        email: row.email.clone(),
        phone: row.phone.clone(),
        note: row.note.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn row(position: Option<&str>) -> ContactRow {
        ContactRow {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            client_id: Uuid::new_v4(),
            full_name: "Мария".into(),
            position_id: position.map(|_| Uuid::new_v4()),
            position_name: position.map(str::to_string),
            email: None,
            phone: None,
            note: None,
        }
    }

    #[test]
    fn a_contact_carries_its_position_name() {
        let dto = contact_to_dto(&row(Some("HR-директор")));
        assert_eq!(dto.position.unwrap().name, "HR-директор");
        assert!(contact_to_dto(&row(None)).position.is_none());
    }
}
