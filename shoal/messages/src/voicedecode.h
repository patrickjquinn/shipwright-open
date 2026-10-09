// Modified by Shipwright, 2026: rebranded as Shoal Messages; see CHANGES-FROM-UPSTREAM.md.
#ifndef VOICEDECODE_H
#define VOICEDECODE_H

/// Selects the decoding helper instead of the app. The environment, not argv:
/// same binary, same jail, no second entry in the desktop file.
#define SHOAL_MESSAGES_VOICE_DECODE_ENV "SHOAL_MESSAGES_VOICE_DECODE"
#define SHOAL_MESSAGES_VOICE_IN_ENV "SHOAL_MESSAGES_VOICE_IN"
#define SHOAL_MESSAGES_VOICE_OUT_ENV "SHOAL_MESSAGES_VOICE_OUT"

/// How the helper ended. The parent reads nothing else from it.
enum VoiceDecodeExit {
    VoiceDecodeOk = 0,
    VoiceDecodeFailed = 1,
    VoiceDecodeNotVoice = 2,
    VoiceDecodeTooLong = 3,
    VoiceDecodeTimedOut = 4,
};

/// Decodes one Ogg/Opus voice message to 16 kHz mono PCM WAV, then exits.
int runVoiceDecode();

#endif // VOICEDECODE_H
