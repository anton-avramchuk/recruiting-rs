use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Recruiter,
    Viewer,
    PlatformAdmin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ManageUsers,
    ReadData,
    WriteData,
    /// Необратимое удаление данных организации (только владелец).
    DeleteData,
    ManageTenants,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::ManageUsers => "manage_users",
            Permission::ReadData => "read_data",
            Permission::WriteData => "write_data",
            Permission::DeleteData => "delete_data",
            Permission::ManageTenants => "manage_tenants",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "manage_users" => Some(Permission::ManageUsers),
            "read_data" => Some(Permission::ReadData),
            "write_data" => Some(Permission::WriteData),
            "delete_data" => Some(Permission::DeleteData),
            "manage_tenants" => Some(Permission::ManageTenants),
            _ => None,
        }
    }

    /// Права, которые можно выдать собственной роли организации.
    pub const GRANTABLE: [Permission; 3] = [
        Permission::ReadData,
        Permission::WriteData,
        Permission::DeleteData,
    ];
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Recruiter => "recruiter",
            Role::Viewer => "viewer",
            Role::PlatformAdmin => "platform_admin",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Role::Owner),
            "recruiter" => Some(Role::Recruiter),
            "viewer" => Some(Role::Viewer),
            "platform_admin" => Some(Role::PlatformAdmin),
            _ => None,
        }
    }

    /// Роли, которые можно назначать пользователям тенанта.
    pub fn is_tenant_role(self) -> bool {
        !matches!(self, Role::PlatformAdmin)
    }

    pub fn permissions(self) -> &'static [Permission] {
        use Permission::*;
        match self {
            Role::Owner => &[ManageUsers, ReadData, WriteData, DeleteData],
            Role::Recruiter => &[ReadData, WriteData],
            Role::Viewer => &[ReadData],
            Role::PlatformAdmin => &[ManageTenants],
        }
    }
}
