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
        brand: {
          bg: '#141414',
          surface: '#1f1f1f',
          panel: '#262626',
          border: '#333333',
          borderLight: '#404040',
          red: '#e50914',
          redHover: '#f40612',
          redDark: '#b91c1c',
          text: '#ffffff',
          textMuted: '#a3a3a3',
          textDim: '#737373',
        },
        adc: {
          // Netflix Charcoal & Red Design System
          bg: '#141414',
          surface: '#1f1f1f',
          panel: '#262626',
          glass: 'rgba(255, 255, 255, 0.06)',
          glassHover: 'rgba(255, 255, 255, 0.12)',
          border: '#333333',
          borderSubtle: '#262626',

          // Netflix Signature Crimson
          crimson: '#e50914',
          crimsonHover: '#f40612',
          crimsonBright: '#e50914',
          crimsonGlow: '#e50914',

          // Typography
          ivory: '#ffffff',
          textMuted: '#a3a3a3',

          // Remapped to 3-color constraint (No rainbow)
          ice: '#ffffff',
          gold: '#e50914',
          emerald: '#ffffff',
          violet: '#e50914',
        }
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'Roboto', 'sans-serif'],
        mono: ['JetBrains Mono', 'ui-monospace', 'SFMono-Regular', 'Menlo', 'Monaco', 'Consolas', 'monospace'],
      },
      boxShadow: {
        'adc-glass': '0 4px 12px rgba(0, 0, 0, 0.5)',
        'adc-dropdown': '0 10px 30px rgba(0, 0, 0, 0.8)',
        'adc-glow': '0 0 14px rgba(229, 9, 20, 0.3)',
      }
    },
  },
  plugins: [],
}
