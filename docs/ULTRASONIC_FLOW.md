# Ultrasonic Flow Measurement

## Overview

Ultrasonic flow measurement is a non-invasive technique that determines fluid flow rate by measuring the time-of-flight (ToF) of ultrasonic waves traveling through the medium. This document describes the physics, implementation, and practical considerations for the UFlowMeter system using TDC1000/TDC7200 converters.

## Fundamental Principles

### Time-of-Flight (ToF) Measurement

The basic principle relies on the relationship between distance, sound velocity, and travel time:

```
Distance = Velocity × Time
d = v × t

Therefore:
t = d / v
```

For bidirectional flow (downstream and upstream), the difference in travel times yields the flow velocity.

### Acoustic Velocity

The speed of ultrasonic waves in a medium depends on:
```
v = √(K / ρ)

Where:
K = bulk modulus (elasticity)
ρ = density

For common media:
- Air (20°C):    343 m/s
- Water (20°C):  1,480 m/s
- Oil (20°C):    1,420 m/s
- Gas (N2):      350 m/s
```

### Measurement Geometry

#### Straight-Pipe Configuration
```
Transducer A (TX)              Transducer B (RX)
      ↓                                ↓
    ┌─────────────────────────────────┐
    │         Flow Direction →        │
    │         (Time: t_downstream)    │
    │  ←     (Time: t_upstream)       │
    └─────────────────────────────────┘
           Distance: d
```

#### V-Path Configuration (Diagonal)
```
    Transducer A                    Transducer B
         (TX)                           (RX)
          ╱                             ╲
         ╱                               ╲
        ╱  t_downstream (with flow)      ╲
       ╱                                   ╲
      ◄─────────────────────────────────────┤
      │      Pipe Cross-Section          │
       ╲                                   ╱
        ╲  t_upstream (against flow)     ╱
         ╲                               ╱
          ╲                             ╱
    Transducer B                    Transducer A
         (RX)                           (TX)
```

## Flow Velocity Calculation

### Dual-Path Method (Doppler Alternative)

The time difference between downstream and upstream measurements yields:

```
t_downstream = d / (v + v_flow)
t_upstream   = d / (v - v_flow)

Where:
d = distance between transducers
v = acoustic velocity in medium
v_flow = flow velocity (what we measure)

Rearranging:
Δt = t_upstream - t_downstream
Δt = d / (v - v_flow) - d / (v + v_flow)
Δt = d × [2v_flow / (v² - v_flow²)]

For v_flow << v (typical case):
Δt ≈ 2d × v_flow / v²

Therefore:
v_flow ≈ (v² × Δt) / (2d)
```

### Flow Rate Calculation

Once velocity is determined, volumetric flow rate:

```
Q = v_flow × A

Where:
A = cross-sectional area of pipe
Q = volumetric flow rate (m³/s, L/min, etc.)

For circular pipe:
A = π × r² = π × (d_pipe/2)²
Q = v_flow × π × (d_pipe/2)²
```

### Mass Flow Rate

For applications requiring mass flow:

```
Q_mass = ρ × Q = ρ × v_flow × A

Where:
ρ = density of the fluid
Q_mass = mass flow rate (kg/s, kg/h, etc.)
```

## Measurement Accuracy Factors

### Temperature Compensation

Sound velocity varies with temperature:

```
For water:
v(T) = 1449 + 4.6×T - 0.055×T²  (T in °C)

Typical variation:
-10°C: v ≈ 1410 m/s
  0°C: v ≈ 1433 m/s
 20°C: v ≈ 1481 m/s
 40°C: v ≈ 1529 m/s

Correction factor:
v_corrected = v_reference × (1 + α × ΔT)

Where α ≈ 0.002 (0.2% per °C for water)
```

### Pressure Compensation

Fluid density changes with pressure:

```
Pressure effect on velocity:
Δv/v ≈ -0.0005 × ΔP_atm  (for water)

Typical correction:
At 1 atm: v = v_reference
At 2 atm: v ≈ 0.9995 × v_reference
```

### Viscosity & Reynolds Number

Flow profile affects measurement:

```
Reynolds Number: Re = (ρ × v × d) / η

Where:
ρ = fluid density
v = flow velocity
d = pipe diameter
η = dynamic viscosity

Laminar flow (Re < 2300):   Non-uniform velocity profile
Turbulent flow (Re > 4000): Approximately uniform profile

Measurement accuracy:
- Laminar: ±2-3% (profile-dependent)
- Turbulent: ±0.5-1% (more uniform)
```

### Pipe Geometry & Calibration Factor

The K-factor relates pulses to volume:

```
K = pulses / volume

For flow measurement:
Q = frequency / K

Determination:
K = n / V

Where:
n = number of pulses during test
V = known volume of fluid collected
Q = flow rate = f / K = (f / n) × V
```

## TDC1000/TDC7200 Considerations

### Time Resolution

```
TDC1000:  250 ps resolution (0.25 ns)
TDC7200:  55 ps resolution (0.055 ns)

Distance resolution (for water):
TDC1000: 250 ps × 1480 m/s ≈ 0.37 mm
TDC7200:  55 ps × 1480 m/s ≈ 0.08 mm
```

### Measurement Range

```
Typical pipe lengths: 0.1 m to 1.0 m

Time-of-flight:
0.1 m:  t ≈ 0.1 m / 1480 m/s ≈ 68 µs
1.0 m:  t ≈ 1.0 m / 1480 m/s ≈ 676 µs

TDC1000 supports up to 800 ns × full scale
TDC7200 has 24-bit counter for extended range
```

### Transducer Frequency Selection

```
Frequency vs. Accuracy/Range Trade-off:

1 MHz:  
  - Wavelength: 1.48 mm
  - Attenuation: ~1 dB/m
  - Accuracy: High
  - Range: Short-medium

2 MHz:
  - Wavelength: 0.74 mm
  - Attenuation: ~4 dB/m
  - Accuracy: Higher
  - Range: Short

40 kHz (ultrasonic):
  - Wavelength: 37 mm
  - Attenuation: ~0.01 dB/m
  - Accuracy: Moderate
  - Range: Long
  - Common for flow applications
```

## System Implementation

### Signal Chain Architecture

```
┌──────────────────────────────────────────┐
│         Ultrasonic Signal Flow            │
└──────────────────────────────────────────┘

Transmit Path:
    Driver Circuit
         ↓
    Transducer TX
         ↓
    Ultrasonic Pulse (40 kHz)
         ↓
    Coupling/Propagation

Receive Path:
    Transducer RX
         ↓
    RX Amplifier (20-40 dB gain)
         ↓
    Bandpass Filter (40 kHz ±2-5 kHz)
         ↓
    Comparator (Signal Squarer)
         ↓
    TDC1000/TDC7200 (ToF Measurement)
         ↓
    Time Value Capture
         ↓
    MCU Processing
```

### TX Driver Circuit

```
Simplified Transducer Driver:

    MCU/Timer Output
         ↓
    ┌────────────┐
    │  Inverter  │  (Generates opposite phase)
    └────────────┘
         ↓
    Complementary Output (MOSFET pair)
         ↓
    Resonant Tank Circuit
         ↓
    Transducer TX
    (40 kHz resonance)
```

### RX Signal Chain

```
Transducer RX (40 kHz)
    ↓
Impedance Matching
    ↓
Low-Noise Preamplifier
    ├─ Gain: 20 dB
    ├─ Noise Figure: < 10 dB
    ├─ Bandwidth: DC to 200 kHz
    └─ Supply: ±5V or 3.3V regulated
    ↓
Bandpass Filter (40 kHz Center)
    ├─ Q: 10-20 (narrow passband)
    ├─ Attenuation: > 40 dB @ 20 kHz, 80 kHz
    ├─ Type: Active 2nd/3rd order
    └─ Ripple: < 1 dB
    ↓
Variable Gain Amplifier (VGA)
    ├─ Gain Range: 0 to 40 dB
    ├─ Control: Digital or analog
    └─ Purpose: Amplitude normalization
    ↓
Comparator/Schmitt Trigger
    ├─ Rising edge detection
    ├─ Hysteresis: 50-100 mV
    └─ Output: Digital square wave
    ↓
TDC1000/TDC7200 Input
```

## Flow Measurement Sequence

### Single Measurement Cycle

```
1. Start Timer (time = 0)
   │
2. Transmit Ultrasonic Pulse (1-5 µs burst)
   │   ├─ Duration: Typically 2-3 cycles @ 40 kHz
   │   └─ Energy: ~10 mJ into transducer
   │
3. Propagation Time (δt)
   │   └─ Downstream: d/(v+v_flow)
   │   └─ Upstream:   d/(v-v_flow)
   │
4. Receive & Detect (10-20 µs window)
   │   ├─ First echo detection (signal exceeds threshold)
   │   ├─ Rising edge crossing
   │   └─ Time capture (t_downstream or t_upstream)
   │
5. Time-of-Flight Result
   │   └─ Δt_downstream or Δt_upstream
   │
6. Repeat in opposite direction
   │
7. Calculate Flow Velocity
    └─ v_flow = (v² × Δt) / (2d)
```

### Dual-Channel Measurement (Recommended)

```
Measurement 1 (Downstream):
    TX-A → RX-B:  t₁ = d/(v + v_flow)
                     ↓
                  CAPTURE

Wait 100-200 ms (settling, temperature stabilization)
                     ↓

Measurement 2 (Upstream):
    TX-B → RX-A:  t₂ = d/(v - v_flow)
                     ↓
                  CAPTURE

Calculate:
    Δt = t₂ - t₁
    v_flow = (v² × Δt) / (2d)
    Q = v_flow × π × r²
```

## Noise and Error Sources

### Acoustic Interference
```
Source: Reflections, multipath propagation
Effect: Delayed echoes, phase shifts
Mitigation:
- Bandpass filtering (±5% of carrier frequency)
- Threshold-based detection (first rising edge)
- Time gating (measurement window)
```

### Electronic Noise
```
Source: Amplifier noise, ground loops, EMI
SNR Target: > 20 dB
Mitigation:
- Low-noise preamp (NF < 10 dB)
- Proper PCB layout (ground plane, star ground)
- Shielded cables for analog signals
- Ferrite filtering on supply
```

### Temperature Drift
```
Effect: ±0.2% per °C in sound velocity
Maximum error over 40°C range: ±8%
Mitigation:
- Temperature sensor (NTC, RTD)
- Real-time compensation: v_corrected = v_ref × (1 + α × ΔT)
- Calibration at multiple temperatures
```

### Transducer Aging
```
Effect: Frequency shift, sensitivity loss
Rate: ~0.05% per year typical
Mitigation:
- Periodic recalibration
- Monitor echo amplitude
- Frequency tracking (PLL if available)
```

## Calibration Procedures

### Static Calibration (No Flow)

```
Objective: Determine system delay (t_offset)

Procedure:
1. No flow condition (blocked or bypass)
2. Measure downstream time: t_d_static
3. Measure upstream time: t_u_static
4. Calculate offset: t_offset = (t_d_static + t_u_static) / 2

Expected: t_offset ≈ d/v (geometric distance/velocity)

Use in measurements:
t_corrected = t_measured - t_offset
```

### Dynamic Calibration (Known Flow)

```
Objective: Determine K-factor or verify accuracy

Setup:
- Known reference flow source
- Calibrated meter or timed collection

Procedure:
1. Set known flow rate (Q_ref)
2. Measure ToF signals (t_downstream, t_upstream)
3. Calculate flow velocity: v_flow_calc
4. Compare with reference: K_actual = v_flow_calc / v_flow_ref
5. Apply correction factor to measurements

Accuracy: Typically ±0.5-2% after calibration
```

### Multi-Point Calibration

```
Improve accuracy across flow range:

Procedure:
1. Collect measurements at 5-10 flow rates
2. Fit polynomial: Q = a₀ + a₁×t + a₂×t² + ...
3. Use fitted coefficients for all measurements

Benefits:
- Accounts for non-linearities
- Reduces systematic errors
- Improves accuracy at extremes
```

## Practical Design Considerations

### Pipe Material Selection

```
Material      Acoustic Impedance    Attenuation    Applications
────────────────────────────────────────────────────────────────
PVC           1.6 MRayl             High           Cold water
Copper        4.7 MRayl             Low            Hot water
Stainless     5.8 MRayl             Low            Industrial
PEX           1.8 MRayl             High           Residential
Cast Iron     3.4 MRayl             Medium         Industrial
```

Impedance matching affects coupling efficiency (power transmitted).

### Transducer Mounting

```
Wet (Inline):
  ┌─ Direct contact with fluid
  ├─ Best accuracy
  ├─ Required cleaning (mineral deposits)
  └─ Typical accuracy: ±1-2%

Clamp-On (Bypass):
  ┌─ External to pipe
  ├─ Mounting window (thin section)
  ├─ No flow resistance
  └─ Typical accuracy: ±2-5%

Wetted (Insertion):
  ┌─ Probe inserted into pipe
  ├─ Good accuracy
  ├─ Pressure handling required
  └─ Typical accuracy: ±1-3%
```

### Distance Optimization

```
Optimal distance: 50-200 mm for 40 kHz transducers

Considerations:
- Shorter distance: Better SNR, less attenuation
- Longer distance: More temperature sensitivity
- Pipe diameter: Distance should be < 80% of diameter

Typical selections:
100 mm pipe:  50-80 mm spacing
50 mm pipe:   25-40 mm spacing
```

### Measurement Frequency

```
Recommended measurement intervals:

Slow-changing flow: 1 Hz or slower
    └─ Suitable for most applications

Dynamic flow (pumps): 10-100 Hz
    ├─ Capture flow variations
    └─ Increased processing load

Turbulent compensation: 100 Hz+
    ├─ Average over turbulent eddies
    └─ MCU/processing dependent
```

## Troubleshooting Common Issues

### No Signal Detection

```
Symptom: ToF measurement fails or returns 0

Causes & Solutions:
1. Transducers disconnected
   → Check impedance with ohmmeter
   → Verify connections

2. Transducer misalignment
   → Realign to face each other
   → Ensure perpendicular orientation

3. Blocked coupling (mineral deposits)
   → Clean transducer surfaces
   → Use soft brush or vinegar soak

4. Driver circuit failure
   → Check TX driver output (oscilloscope)
   → Verify capacitor values in tank circuit

5. Receive amplifier not working
   → Check power supply (+5V or ±5V)
   → Measure preamp output (oscilloscope)
   → Verify filter tuning (40 kHz)
```

### Noisy/Unstable Readings

```
Symptom: Large variation in consecutive measurements (>2%)

Causes & Solutions:
1. Acoustic reflections
   → Improve bandpass filter selectivity
   → Add acoustic foam or damping
   → Verify single-path propagation

2. Electronic noise
   → Check ground connections
   → Add ferrite filtering on supply
   → Shield analog signal cables
   → Move away from RF sources

3. Temperature fluctuation
   → Stabilize ambient temperature
   → Allow 30 min warmup time
   → Apply temperature compensation

4. Flow instability in pipe
   → Move sensors upstream of valves/elbows
   → Allow straight pipe (10× diameter upstream)
   → Check for flow swirls

5. Insufficient signal amplitude
   → Increase TX driver power
   → Check RX amplifier gain
   → Verify impedance matching
```

### Incorrect Flow Rate

```
Symptom: Consistently wrong flow values (±5-10% or more)

Causes & Solutions:
1. Incorrect distance calibration
   → Measure physical pipe distance precisely
   → Update d parameter in firmware
   → Recalibrate K-factor

2. Temperature not compensated
   → Add temperature sensor
   → Apply v_corrected = v_ref × (1 + α × ΔT)
   → Recalibrate at different temperatures

3. Wrong fluid properties
   → Verify acoustic velocity for actual fluid
   → Check fluid composition (water vs. glycol mix)
   → Adjust K-factor accordingly

4. Multipath propagation
   → Longer pulses than necessary
   → Use time gating (gate only first arrival)
   → Improve signal detection threshold

5. Pipe geometry issues
   → Verify straight pipe section
   → Check for bends near sensors
   → Ensure proper entrance conditions (turbulent)
```

## Performance Optimization

### Measurement Averaging

```
Reduce noise through temporal averaging:

N-point moving average:
Q_avg(n) = (Q(n) + Q(n-1) + ... + Q(n-N+1)) / N

Typical N values:
- 5 samples (quiet environment): Reduces noise ~50%
- 10 samples (moderate noise): Reduces noise ~68%
- 20 samples (noisy): Reduces noise ~78%

Latency trade-off:
N=5:  Latency = 5×T_meas
N=10: Latency = 10×T_meas
N=20: Latency = 20×T_meas
```

### Adaptive Gain Control

```
Automatic amplitude normalization:

1. Measure peak signal amplitude
2. Compare to target (e.g., 80% full scale)
3. Adjust VGA gain: G_new = G_old × (A_target / A_actual)
4. Repeat measurement with new gain

Benefits:
- Maintains SNR over temperature/aging
- Reduces threshold sensitivity
- Improves timing accuracy
```

## Advanced Topics

### Doppler Effect (Alternative Method)

```
Frequency shift method (rarely used for flow):

f_observed = f_transmitted × (v_medium + v_observer) / (v_medium + v_source)

For flow measurement:
f_shift ≈ 2 × f_transmitted × v_flow / v_medium

Advantages:
- No need for dual transducers
- Inherently measures velocity

Disadvantages:
- Requires frequency measurement (more complex)
- Lower accuracy in liquids
- Phase measurement critical
```

### Correlation Flow Meters

```
Alternative using cross-correlation:

1. Transmit wide-spectrum signal or noise burst
2. Capture signals at two points (few meters apart)
3. Cross-correlate the two signals
4. Peak correlation time = transit time

Less affected by:
- Signal attenuation
- Transducer aging
- Temperature variations

Higher accuracy but more processing
```

## Summary

Key principles for accurate ultrasonic flow measurement:

1. **Geometry**: Accurately measure pipe distance and diameter
2. **Calibration**: Static and dynamic calibration essential
3. **Temperature**: Compensate for velocity variation (±0.2%/°C)
4. **Signal Quality**: Bandpass filter, adequate amplification, threshold detection
5. **Time Resolution**: TDC7200 (55 ps) preferred over TDC1000 (250 ps)
6. **Measurement Rate**: Balance between latency and noise reduction
7. **Environmental**: Shield from EMI, stable power supply
8. **Validation**: Compare with reference meter during commissioning

Typical system accuracy: **±1-3%** after proper calibration and compensation.

## References

- ISO 6416: Measurement of liquid flow in open channels using ultrasonic methods
- IEC 60874: Guidance on measurement techniques for ultrasonic flow measurement
- TI TDC1000/TDC7200 Datasheets
- "Acoustic Fundamentals" - Heinrich Kuttruff
- "Ultrasonic Transducers" - Karl F. Graff

