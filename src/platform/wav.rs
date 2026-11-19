//! WAV bytes ↔ 16-bit mono samples. Shared by the native player (reads a
//! file) and the web recorder/player (bytes kept in IndexedDB).

use std::io::{Read, Seek, Write};

use super::{AudioError, err};

/// Reads 16-bit PCM WAV, mixing any channels down to mono.
pub fn decode_mono_i16(source: impl Read) -> Result<(Vec<i16>, u32), AudioError> {
    let mut reader = hound::WavReader::new(source).map_err(|e| err("Could not open the recording", e))?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(AudioError("This recording is not in Lectio's 16-bit format.".into()));
    }
    let raw: Vec<i16> =
        reader.samples::<i16>().collect::<Result<_, _>>().map_err(|e| err("The recording is damaged", e))?;
    let channels = usize::from(spec.channels.max(1));
    let mono = raw
        .chunks(channels)
        .map(|frame| (frame.iter().map(|&s| i32::from(s)).sum::<i32>() / frame.len() as i32) as i16)
        .collect();
    Ok((mono, spec.sample_rate))
}

/// The WAV format Lectio stores (ADR-005).
pub fn mono_spec(sample_rate: u32) -> hound::WavSpec {
    hound::WavSpec { channels: 1, sample_rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }
}

/// Writes mono 16-bit samples as a complete WAV into `sink`. The web
/// recorder uses it (native writes the file as it records); it is unit-tested
/// on every target.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn encode_mono_i16(samples: &[i16], sample_rate: u32, sink: impl Write + Seek) -> Result<(), AudioError> {
    let mut writer =
        hound::WavWriter::new(sink, mono_spec(sample_rate)).map_err(|e| err("Encoding audio failed", e))?;
    for &sample in samples {
        writer.write_sample(sample).map_err(|e| err("Encoding audio failed", e))?;
    }
    writer.finalize().map_err(|e| err("Encoding audio failed", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn encoded_wav_decodes_to_the_same_samples() {
        let samples = vec![0, 1, -1, i16::MAX, -i16::MAX, 1234];
        let mut bytes = Cursor::new(Vec::new());

        encode_mono_i16(&samples, 16_000, &mut bytes).unwrap();
        let (decoded, rate) = decode_mono_i16(Cursor::new(bytes.into_inner())).unwrap();

        assert_eq!(decoded, samples);
        assert_eq!(rate, 16_000);
    }

    #[test]
    fn non_wav_bytes_are_an_error_not_a_panic() {
        assert!(decode_mono_i16(Cursor::new(b"not a wav".to_vec())).is_err());
    }
}
