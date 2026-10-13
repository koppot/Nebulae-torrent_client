/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        adc: {
          // ADC watch.php and winter dark red theme tokens
          bg: '#000000',
          surface: '#0a0a0c',
          panel: 'rgba(20, 20, 24, 0.95)',
          glass: 'rgba(255, 255, 255, 0.08)',
          glassHover: 'rgba(255, 255, 255, 0.16)',
          border: 'rgba(255, 255, 255, 0.15)',
          borderSubtle: 'rgba(255, 255, 255, 0.08)',

          // ADC Signature Crimson Red (from watch.php rgba(132, 0, 1, 0.85))
          crimson: '#840001',
          crimsonHover: '#980e10',
          crimsonBright: '#b01214',
          crimsonGlow: '#d22b2f',

          // Typography
          ivory: '#ECE9D8',
          textMuted: 'rgba(255, 255, 255, 0.72)',

          // Secondary Telemetry & Status accents
          ice: '#68e0ff',
          gold: '#f59e0b',
          emerald: '#10b981',
          violet: '#8b5cf6',
        }
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'Roboto', 'sans-serif'],
        mono: ['JetBrains Mono', 'ui-monospace', 'SFMono-Regular', 'Menlo', 'Monaco', 'Consolas', 'monospace'],
      },
      boxShadow: {
        'adc-glass': '0 4px 12px rgba(0, 0, 0, 0.6)',
        'adc-dropdown': '0 10px 30px rgba(0, 0, 0, 0.8)',
        'adc-glow': '0 0 14px rgba(210, 43, 47, 0.4)',
      }
    },
  },
  plugins: [],
}
