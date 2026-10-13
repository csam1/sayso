import { useEffect, useRef, useState, type PointerEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

const BAR_COUNT = 24;
const SAMPLE_INTERVAL_MS = 50;
const IDLE_LEVEL = 0.05;
const CAPTURE_BUFFER_SIZE = 4096;

type Phase = "recording" | "transcribing";

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [phase, setPhase] = useState<Phase>("recording");
  const [levels, setLevels] = useState<number[]>(() =>
    new Array(BAR_COUNT).fill(IDLE_LEVEL),
  );

  const streamRef = useRef<MediaStream | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const intervalRef = useRef<number | null>(null);
  const chunksRef = useRef<Float32Array[]>([]);
  const sampleRateRef = useRef(48000);
  const stoppingRef = useRef(false);
  const handleStopRef = useRef<() => void>(() => {});

  const stopCapture = () => {
    if (intervalRef.current !== null) {
      window.clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
    processorRef.current?.disconnect();
    processorRef.current = null;
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
    audioContextRef.current?.close().catch(() => {});
    audioContextRef.current = null;
  };

  useEffect(() => {
    const unlisten = listen<boolean>("recording-state-changed", (event) => {
      setIsRecording(event.payload);
      if (event.payload) {
        stoppingRef.current = false;
        setPhase("recording");
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen("stop-requested", () => {
      handleStopRef.current();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!isRecording) {
      return;
    }

    let cancelled = false;
    chunksRef.current = [];

    (async () => {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        if (cancelled) {
          stream.getTracks().forEach((track) => track.stop());
          return;
        }

        streamRef.current = stream;
        const audioContext = new AudioContext();
        audioContextRef.current = audioContext;
        sampleRateRef.current = audioContext.sampleRate;

        const source = audioContext.createMediaStreamSource(stream);

        const analyser = audioContext.createAnalyser();
        analyser.fftSize = 256;
        source.connect(analyser);

        const data = new Uint8Array(analyser.frequencyBinCount);

        intervalRef.current = window.setInterval(() => {
          analyser.getByteTimeDomainData(data);

          let sumSquares = 0;
          for (let i = 0; i < data.length; i++) {
            const normalized = (data[i] - 128) / 128;
            sumSquares += normalized * normalized;
          }
          const rms = Math.sqrt(sumSquares / data.length);
          const amplitude = Math.max(IDLE_LEVEL, Math.min(1, rms * 4));

          setLevels((prev) => [...prev.slice(1), amplitude]);
        }, SAMPLE_INTERVAL_MS);

        const muteNode = audioContext.createGain();
        muteNode.gain.value = 0;

        const processor = audioContext.createScriptProcessor(
          CAPTURE_BUFFER_SIZE,
          1,
          1,
        );
        processorRef.current = processor;
        source.connect(processor);
        processor.connect(muteNode);
        muteNode.connect(audioContext.destination);
        processor.onaudioprocess = (event) => {
          chunksRef.current.push(
            new Float32Array(event.inputBuffer.getChannelData(0)),
          );
        };
      } catch (error) {
        console.error("Microphone capture failed:", error);
      }
    })();

    return () => {
      cancelled = true;
      stopCapture();
      setLevels(new Array(BAR_COUNT).fill(IDLE_LEVEL));
    };
  }, [isRecording]);

  const handleDragStart = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    getCurrentWindow().startDragging();
  };

  const handleStop = async () => {
    if (stoppingRef.current) return;
    stoppingRef.current = true;

    setPhase("transcribing");
    stopCapture();

    const totalLength = chunksRef.current.reduce(
      (sum, chunk) => sum + chunk.length,
      0,
    );
    const samples = new Float32Array(totalLength);
    let offset = 0;
    for (const chunk of chunksRef.current) {
      samples.set(chunk, offset);
      offset += chunk.length;
    }
    chunksRef.current = [];

    let transcript: string | null = null;
    try {
      transcript = await invoke<string>("transcribe_recording", {
        samples: Array.from(samples),
        sampleRate: sampleRateRef.current,
      });
    } catch (error) {
      console.error("Transcription failed:", error);
    } finally {
      await invoke("stop_recording", { transcript });
    }
  };
  handleStopRef.current = handleStop;

  return (
    <div className="overlay" onPointerDown={handleDragStart}>
      {phase === "recording" ? (
        <div className="bars" aria-hidden="true">
          {levels.map((level, index) => (
            <span
              key={index}
              className="bar"
              style={{ height: `${8 + level * 32}px` }}
            />
          ))}
        </div>
      ) : (
        <div className="status-label">Transcribing…</div>
      )}
      <button
        type="button"
        className="stop-button"
        onPointerDown={(event) => event.stopPropagation()}
        onClick={handleStop}
        aria-label="Stop recording"
        disabled={phase === "transcribing"}
      >
        <span className="stop-icon" />
      </button>
    </div>
  );
}

export default App;
