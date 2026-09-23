# ADR 2.2: Transcribe file bằng Gemini generateContent

**Status:** Direction accepted; live S2 request pending.
**Date:** 2026-09-23

## Decision

Theo điều chỉnh của chủ sản phẩm, transcribe file dùng model Gemini thông thường qua `POST /v1beta/models/{model}:generateContent` với FLAC inline và JSON Segment. Prompt yêu cầu tự nhận diện tiếng Việt/tiếng Nhật xen kẽ, giữ nguyên lời nói và không dịch. Model `*-transcribe`/Interactions và Files API không thuộc đường transcribe file v3. Chế độ Live dùng Live Translate qua WebSocket để nhận transcript đầu vào, transcript bản dịch và audio bản dịch; luồng đó thuộc Epic 4–5.

Tên model version đã biết có thinking config tương ứng. Alias mặc định `gemini-flash-lite-latest` và tên người dùng nhập được gửi nguyên tên với thinking mặc định của model, không suy ra major version hay tự đổi model. Google sẽ trả lỗi nếu model không hỗ trợ JSON schema hoặc audio inline.

## Evidence

- Code v2 ở `../transcriber-kun/app/python/transcriber.py` gọi `client.models.generate_content` với prompt và JSON schema cho model thông thường; `../transcriber-kun/app/python/realtime_gemini.py` dùng WebSocket BidiGenerateContent cho Live.
- [Gemini audio guide](https://ai.google.dev/gemini-api/docs/generate-content/audio) mô tả audio inline cho generateContent dưới giới hạn request 20 MB.
- [Live Translate guide](https://ai.google.dev/gemini-api/docs/live-api/live-translate) mô tả `inputAudioTranscription`, `outputAudioTranscription` và audio output trong Live session.

## S2 verification remaining

Môi trường triển khai không có Gemini API key, nên chưa có request thật với model được chọn và fixture Việt–Nhật. Trước khi chốt S2, chạy FLAC inline 5 phút qua `generateContent`, xác nhận JSON schema, kích thước payload và so timestamp với v2; ghi model ID, ngày, trạng thái và số đo mà không lưu key/audio/transcript trong ADR.
