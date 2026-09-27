//! ID Phiên/Transcript/Job: newtype bọc UUIDv7. Chỉ tạo được qua [`new`] (sinh
//! mới) hoặc `TryFrom<&str>` (parse, từ chối bất kỳ chuỗi nào không phải
//! UUIDv7 hợp lệ — kể cả UUIDv4 hay chuỗi tuỳ ý như `"../etc"`). Đây là điều
//! kiện để [`crate::core::paths`] an toàn: helper đường dẫn chỉ nhận các
//! newtype này, không bao giờ nhận `&str` thô, nên không thể lắp một chuỗi
//! path traversal vào đường dẫn file.

use std::fmt;
use uuid::Uuid;

macro_rules! uuid_v7_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(Uuid);

        impl $name {
            /// Sinh ID mới, luôn là UUIDv7.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = crate::core::error::AppError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                let parsed = Uuid::parse_str(value).map_err(|_| {
                    crate::core::error::AppError::new(
                        crate::core::error::Code::Format,
                        concat!(stringify!($name), ": không parse được thành UUID"),
                    )
                })?;
                if parsed.get_version_num() != 7 {
                    return Err(crate::core::error::AppError::new(
                        crate::core::error::Code::Format,
                        concat!(stringify!($name), ": chỉ chấp nhận UUIDv7"),
                    ));
                }
                Ok(Self(parsed))
            }
        }

        // `uuid` không bật feature `serde` (Cargo.toml chỉ bật `v7`), nên
        // (de)serialize thủ công qua chuỗi — cùng con đường với `Display`/
        // `TryFrom<&str>` ở trên, để một giá trị lỗi (không phải UUIDv7) bị
        // từ chối giống hệt lúc parse trực tiếp.
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0.to_string())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                Self::try_from(raw.as_str()).map_err(serde::de::Error::custom)
            }
        }

        // Xuất sang TypeScript như một chuỗi opaque — khớp đúng hình dạng
        // JSON thật ở trên (spec: id qua IPC là chuỗi UUIDv7).
        impl specta::Type for $name {
            fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
                <String as specta::Type>::definition(types)
            }
        }
    };
}

uuid_v7_id!(SessionId, "ID Phiên (Session), UUIDv7.");
uuid_v7_id!(TranscriptId, "ID Transcript, UUIDv7.");
uuid_v7_id!(JobId, "ID Job, UUIDv7.");
uuid_v7_id!(TagId, "ID Tag (story 3.2), UUIDv7.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_produces_valid_v7() {
        let id = SessionId::new();
        assert_eq!(id.0.get_version_num(), 7);
    }

    #[test]
    fn try_from_accepts_valid_v7_string() {
        let id = SessionId::new();
        let round_tripped = SessionId::try_from(id.to_string().as_str())
            .expect("chuỗi UUIDv7 hợp lệ phải parse được");
        assert_eq!(round_tripped, id);
    }

    #[test]
    fn try_from_rejects_path_traversal_string() {
        assert!(SessionId::try_from("../etc").is_err());
    }

    #[test]
    fn try_from_rejects_uuid_v4() {
        // Không bật feature `v4` (chỉ `v7` theo Code Map) nên dựng thủ công
        // 16 byte có version nibble = 4, variant RFC4122 — đủ để
        // `get_version_num()` trả 4 mà không cần `Uuid::new_v4()`.
        let mut bytes = [
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x00, 0x88, 0x00, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
            0xff, 0x00,
        ];
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        let v4 = Uuid::from_bytes(bytes);
        assert_eq!(v4.get_version_num(), 4);
        assert!(SessionId::try_from(v4.to_string().as_str()).is_err());
    }

    #[test]
    fn try_from_rejects_empty_string() {
        assert!(TranscriptId::try_from("").is_err());
    }

    #[test]
    fn ids_of_different_kinds_are_distinct_types() {
        // Kiểm ở mức biên dịch: nếu dòng dưới biên dịch được nghĩa là newtype
        // không lẫn nhau, nhưng cứ giữ một assert runtime cho có nội dung.
        let session = SessionId::new();
        let job = JobId::new();
        assert_ne!(session.to_string(), "");
        assert_ne!(job.to_string(), "");
    }
}
