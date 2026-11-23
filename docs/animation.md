# Animation System (`pastel_core` & `pastel_render.anim_state`)

## 1. Pluggable `Curve` Trait

All animations are built on `pastel_core.curve`:

- **`CurveSpec`**: Configured directly from TOML as either:
  - `Spring { stiffness: f64, damping: f64 }` (where `damping` is the damping ratio $\zeta$: `1.0` = critically damped, `< 1.0` = subtle bounce, `> 1.0` = overdamped)
  - `Ease { ms: f64, kind: EaseKind }` (`linear`, `out-quad`, `out-cubic`, `out-quint`, `out-expo`, `in-out-cubic`)
- **Global `slowdown`** multiplier (`[animation].slowdown`) scales duration / angular frequency uniformly without altering curve shape.

## 2. Exact Analytical Spring Solver

Instead of Euler or RK4 numerical stepping (which drifts across variable frame intervals and VRR monitors), `pastel_core` solves the damped harmonic oscillator analytically in O(1) time for any elapsed seconds $t$:

$$\ddot{x} + 2\zeta\omega_0\dot{x} + \omega_0^2(x - x_{\text{target}}) = 0, \quad \omega_0 = \sqrt{\text{stiffness}}$$

Given initial displacement $x_0 = x(0) - x_{\text{target}}$ and velocity $v_0 = \dot{x}(0)$:

- **Critically damped ($\zeta \approx 1$)**:
  $$x(t) = x_{\text{target}} + e^{-\omega_0 t}\bigl(x_0 + (v_0 + \omega_0 x_0)t\bigr)$$
- **Under-damped ($\zeta < 1$)**:
  $$\omega_d = \omega_0\sqrt{1-\zeta^2},\quad x(t) = x_{\text{target}} + e^{-\zeta\omega_0 t}\left(x_0\cos(\omega_d t) + \frac{v_0 + \zeta\omega_0 x_0}{\omega_d}\sin(\omega_d t)\right)$$
- **Over-damped ($\zeta > 1$)**:
  $$r_{1,2} = -\omega_0(\zeta \mp \sqrt{\zeta^2 - 1}),\quad x(t) = x_{\text{target}} + c_1 e^{r_1 t} + c_2 e^{r_2 t}$$

Both position $x(t)$ and exact velocity $\dot{x}(t)$ are evaluated in closed form.

## 3. Momentum-Preserving In-Flight Retargeting

When a user rapidly taps `Mod+L` three times mid-scroll, `Anim.set_target(new_target, now_ns)`:

1. Evaluates the exact current `(pos, vel)` at `now_ns`.
2. Resets the analytical curve origin to `(pos, vel)` with target `new_target` at `t0 = now_ns`.
3. Never snaps, jerks, or loses velocity.

An animation settles to its exact target once $|x(t) - x_{\text{target}}| < \epsilon_x$ and $|\dot{x}(t)| < \epsilon_v$, guaranteeing clean termination and returning the compositor to zero-wakeup idle.

## 4. Animated Channels

- **Window geometry & strip scroll** (`window-movement`): X, Y, W, H per window (smoothly interpolating column/camera shifts)
- **Workspace switch** (`workspace-switch`): vertical strip slide between workspaces
- **Window open / close** (`window-open` / `window-close`): subtle scale (`0.94 <-> 1.0`) and opacity fade on open and close
- **Focus ring & bell pulse** (`focus-ring`): RGBA border color interpolation between `inactive-color` and `active-color`, plus a 350 ms highlight pulse on `xdg-system-bell-v1`
- **Hotkey overlay** (`overlay`): opacity & slight vertical slide (`12px`)
- **Overview** (`overview`): smooth scale & position zoom between active workspace and multi-workspace thumbnail bands (`0.0 <-> 1.0`)
