<CsoundSynthesizer>
<CsOptions>
-d -m0 -W -3 -o evidence-knot-palette.wav
</CsOptions>
<CsInstruments>
sr = 48000
ksmps = 32
nchnls = 2
0dbfs = 1
seed 11871

instr Contact
  iFrequency = p4
  iAmplitude = p5
  iPan = p6
  aEnvelope expseg 0.001, 0.004, 1, p3 - 0.034, 0.002, 0.03, 0.0001
  aCore poscil iAmplitude * aEnvelope, iFrequency
  aWood poscil iAmplitude * 0.22 * aEnvelope, iFrequency * 2.013
  aSignal = aCore + aWood
  outs aSignal * sqrt(1 - iPan), aSignal * sqrt(iPan)
endin

instr Thread
  iPanStart = p4
  iPanEnd = p5
  aNoise rand 0.13
  aShape linsegr 0, 0.08, 1, p3 - 0.20, 0.72, 0.12, 0
  aThread butbp aNoise, 1750, 620
  kPan linseg iPanStart, p3, iPanEnd
  outs aThread * aShape * sqrt(1 - kPan), aThread * aShape * sqrt(kPan)
endin

instr Harmonic
  iRoot = p4
  iAmplitude = p5
  aEnvelope linsegr 0, 0.10, 1, p3 - 0.42, 0.72, 0.32, 0
  aSignal = poscil(iAmplitude, iRoot) + poscil(iAmplitude * 0.55, iRoot * 1.5) + poscil(iAmplitude * 0.32, iRoot * 2)
  outs aSignal * aEnvelope, aSignal * aEnvelope
endin
</CsInstruments>
<CsScore>
; memory-stitch: 0.00–1.40
i "Contact" 0.00 0.24 220 0.18 0.50
i "Thread"  0.12 0.88 0.44 0.56
i "Harmonic" 0.18 1.18 220 0.045
; clock-prism: 2.00–3.35
i "Contact" 2.00 0.22 220 0.15 0.46
i "Contact" 2.36 0.22 330 0.13 0.50
i "Contact" 2.83 0.42 440 0.11 0.54
; relation-thread: 4.00–5.25
i "Thread" 4.00 1.20 0.40 0.60
; evidence-open: 6.00–6.52. The short tail is deliberate: master 1 enters
; digital silence at 13.20 after placing this cue at 12.60.
i "Harmonic" 6.00 0.50 220 0.052
i "Thread" 6.05 0.42 0.48 0.52
; proof hops: 8.00–11.55
i "Contact" 8.00 0.42 220 0.14 0.46
i "Thread"  8.05 0.34 0.46 0.50
i "Contact" 9.00 0.42 247 0.14 0.48
i "Thread"  9.05 0.34 0.48 0.50
i "Contact" 10.00 0.42 277 0.14 0.52
i "Thread"  10.05 0.34 0.50 0.52
i "Contact" 11.00 0.42 330 0.14 0.54
i "Thread"  11.05 0.34 0.50 0.54
; evidence-knot: 12.00–14.28
i "Contact" 12.00 0.24 220 0.13 0.46
i "Contact" 12.36 0.24 330 0.12 0.50
i "Contact" 12.83 0.42 440 0.11 0.54
i "Thread"  12.15 1.55 0.42 0.58
i "Harmonic" 12.64 1.64 220 0.046
; retained-turn: 15.00–16.85
i "Harmonic" 15.00 1.80 196 0.038
i "Contact"  15.15 0.34 220 0.10 0.48
i "Harmonic" 15.58 1.20 247 0.030
; strands and WAL convergence: 18.00–21.60
i "Thread" 18.00 0.68 0.30 0.42
i "Thread" 19.00 0.68 0.70 0.58
i "Thread" 20.00 1.55 0.28 0.50
i "Thread" 20.00 1.55 0.72 0.50
i "Harmonic" 20.38 1.18 220 0.034
e
</CsScore>
</CsoundSynthesizer>
