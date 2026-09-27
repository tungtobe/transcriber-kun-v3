//! Hằng số Template memo mặc định (story 3.6, spec Boundaries Decision OQ8:
//! "Văn bản chính xác nằm trong một file hằng số Rust và là nguồn duy nhất").
//! Mỗi locale (`vi`/`en`/`ja`) có đúng 2 mẫu: `meeting-minutes` (biên bản
//! họp một ngôn ngữ) và `bilingual-ja-vi` (memo song ngữ Nhật–Việt). Không có
//! nơi thứ hai trong codebase chứa văn bản prompt này -- `memo::templates`
//! chỉ đọc qua [`defaults_for_locale`].

/// Một mẫu mặc định trước khi có `id`/timestamp -- ghép với chúng ở
/// `memo::templates` lúc seed/khôi phục.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultTemplate {
    pub default_key: &'static str,
    pub name: &'static str,
    pub prompt: &'static str,
}

const MEETING_MINUTES_VI: DefaultTemplate = DefaultTemplate {
    default_key: "meeting-minutes",
    name: "Biên bản họp",
    prompt: "Bạn là một người ghi biên bản họp (thư ký cuộc họp) chuyên nghiệp. Dựa vào \
transcript cuộc họp dưới đây (và ghi chú của người tham dự, nếu có), hãy viết một bản biên \
bản họp bằng tiếng Việt, trình bày dưới dạng Markdown với đúng các mục sau:\n\n\
## Tóm tắt\n\
## Nội dung chính đã thảo luận\n\
## Quyết định\n\
## Việc cần làm\n\
(Ghi rõ người phụ trách và hạn hoàn thành nếu có trong transcript hoặc ghi chú)\n\
## Vấn đề còn mở\n\n\
Không được bịa thêm bất kỳ thông tin nào không có trong transcript hoặc ghi chú. Chỉ trả về \
đúng nội dung biên bản theo các mục trên, không thêm lời chào, lời dẫn hay giải thích nào \
khác.\n\n\
Ghi chú:\n\
{notes}\n\n\
Transcript:\n\
{transcript}",
};

const BILINGUAL_JA_VI_VI: DefaultTemplate = DefaultTemplate {
    default_key: "bilingual-ja-vi",
    name: "Memo song ngữ Nhật–Việt",
    prompt: "Bạn là trợ lý viết memo cuộc họp song ngữ Nhật–Việt. Dựa vào transcript cuộc họp \
dưới đây (và ghi chú nếu có), hãy tạo hai memo riêng biệt, đầy đủ và độc lập với nhau (không \
phải bản dịch từng câu), tóm tắt lại nội dung chính, quyết định và việc cần làm của cuộc họp:\n\n\
## メモ（日本語）\n\
Viết một memo hoàn chỉnh bằng tiếng Nhật, tự nhiên, đúng văn phong biên bản công việc của \
người Nhật.\n\n\
## Memo (Tiếng Việt)\n\
Viết một memo hoàn chỉnh bằng tiếng Việt, rõ ràng, dễ đọc, nội dung tương đương với memo \
tiếng Nhật ở trên.\n\n\
Không bịa thêm thông tin không có trong transcript hoặc ghi chú. Chỉ trả về nội dung hai memo \
theo đúng hai mục trên, không thêm lời dẫn hay giải thích nào khác.\n\n\
Ghi chú:\n\
{notes}\n\n\
Transcript:\n\
{transcript}",
};

const MEETING_MINUTES_EN: DefaultTemplate = DefaultTemplate {
    default_key: "meeting-minutes",
    name: "Meeting minutes",
    prompt: "You are a professional meeting minutes writer. Based on the meeting transcript \
below (and the participants' notes, if any), write meeting minutes in English, formatted as \
Markdown with exactly these sections:\n\n\
## Summary\n\
## Key points discussed\n\
## Decisions\n\
## Action items\n\
(State the owner and due date for each item when available in the transcript or notes)\n\
## Open issues\n\n\
Do not invent any information that is not present in the transcript or notes. Return only the \
minutes content in the sections above, with no greeting, preamble, or extra explanation.\n\n\
Notes:\n\
{notes}\n\n\
Transcript:\n\
{transcript}",
};

const BILINGUAL_JA_VI_EN: DefaultTemplate = DefaultTemplate {
    default_key: "bilingual-ja-vi",
    name: "Japanese–Vietnamese memo",
    prompt: "You are an assistant that writes bilingual Japanese–Vietnamese meeting memos. \
Based on the meeting transcript below (and the notes, if any), create two separate, complete \
memos (not a sentence-by-sentence translation of each other) summarizing the key points, \
decisions, and action items of the meeting:\n\n\
## メモ（日本語）\n\
Write a complete memo in natural Japanese, in the style of a Japanese business memo.\n\n\
## Memo (Tiếng Việt)\n\
Write a complete memo in clear, readable Vietnamese, covering the same content as the \
Japanese memo above.\n\n\
Do not invent any information that is not present in the transcript or notes. Return only the \
two memos in the sections above, with no greeting, preamble, or extra explanation.\n\n\
Notes:\n\
{notes}\n\n\
Transcript:\n\
{transcript}",
};

const MEETING_MINUTES_JA: DefaultTemplate = DefaultTemplate {
    default_key: "meeting-minutes",
    name: "議事録",
    prompt: "あなたはプロの議事録作成者です。以下の会議のトランスクリプト（および参加者のメモが\
あればそれも参照して）をもとに、日本語で議事録を作成してください。Markdown形式で、必ず次の見出し\
を使ってください。\n\n\
## 概要\n\
## 話し合われた主な内容\n\
## 決定事項\n\
## 対応事項（ToDo）\n\
（担当者と期限が分かる場合は明記してください）\n\
## 未解決の課題\n\n\
トランスクリプトやメモにない情報を創作しないでください。上記の見出しに沿った議事録の内容のみを返\
し、挨拶や前置き、説明文は付けないでください。\n\n\
メモ:\n\
{notes}\n\n\
トランスクリプト:\n\
{transcript}",
};

const BILINGUAL_JA_VI_JA: DefaultTemplate = DefaultTemplate {
    default_key: "bilingual-ja-vi",
    name: "日越バイリンガルメモ",
    prompt: "あなたは日本語とベトナム語のバイリンガル会議メモを作成するアシスタントです。以下の\
会議のトランスクリプト（およびメモがあればそれも参照して）をもとに、内容・決定事項・対応事項を要約\
した、独立した2つのメモを作成してください（一方をもう一方の逐語訳にはしないでください）。\n\n\
## メモ（日本語）\n\
自然な日本語で、ビジネスメモらしい文体で完全なメモを書いてください。\n\n\
## Memo (Tiếng Việt)\n\
上のメモと同じ内容を、分かりやすいベトナム語で完全なメモとして書いてください。\n\n\
トランスクリプトやメモにない情報を創作しないでください。上記2つのメモの内容のみを返し、挨拶や前\
置き、説明文は付けないでください。\n\n\
メモ:\n\
{notes}\n\n\
トランスクリプト:\n\
{transcript}",
};

/// Hai mẫu mặc định của một locale, theo đúng thứ tự cố định
/// (`meeting-minutes` rồi `bilingual-ja-vi`) -- `None` khi `locale` không
/// phải `vi`/`en`/`ja` (spec Code Map: "Locale nhận chuỗi `vi|en|ja`, giá trị
/// khác → lỗi `Request`", xử lý ở `memo::templates`, hàm này chỉ tra bảng).
pub fn defaults_for_locale(locale: &str) -> Option<[DefaultTemplate; 2]> {
    match locale {
        "vi" => Some([MEETING_MINUTES_VI, BILINGUAL_JA_VI_VI]),
        "en" => Some([MEETING_MINUTES_EN, BILINGUAL_JA_VI_EN]),
        "ja" => Some([MEETING_MINUTES_JA, BILINGUAL_JA_VI_JA]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUPPORTED_LOCALES: [&str; 3] = ["vi", "en", "ja"];

    #[test]
    fn every_supported_locale_has_two_defaults_with_the_expected_keys() {
        for locale in SUPPORTED_LOCALES {
            let defs = defaults_for_locale(locale)
                .unwrap_or_else(|| panic!("thiếu mặc định cho {locale}"));
            assert_eq!(defs[0].default_key, "meeting-minutes");
            assert_eq!(defs[1].default_key, "bilingual-ja-vi");
        }
    }

    #[test]
    fn unsupported_locale_returns_none() {
        assert!(defaults_for_locale("fr").is_none());
        assert!(defaults_for_locale("").is_none());
    }

    /// Story 3.6 spec Tasks: "mọi mặc định chứa `{transcript}` và `{notes}`".
    #[test]
    fn every_default_prompt_contains_both_placeholders_exactly_once() {
        for locale in SUPPORTED_LOCALES {
            for def in defaults_for_locale(locale).unwrap() {
                assert_eq!(
                    def.prompt.matches("{transcript}").count(),
                    1,
                    "{} / {}: phải chứa đúng một {{transcript}}",
                    locale,
                    def.default_key
                );
                assert_eq!(
                    def.prompt.matches("{notes}").count(),
                    1,
                    "{} / {}: phải chứa đúng một {{notes}}",
                    locale,
                    def.default_key
                );
                assert!(!def.name.trim().is_empty());
            }
        }
    }
}
