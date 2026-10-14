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
        // docs/ui-design.md official color tokens
        'adc-dark': '#0B0F19',
        'adc-cardDark': '#131B2E',
        'adc-cardBorderDark': '#1E293B',
        'adc-accent': '#EC4899',
        'adc-gold': '#F59E0B',
        'adc-cyan': '#06B6D4',
        'adc-emerald': '#10B981',
        'adc-violet': '#8B5CF6',
        adc: {
          dark: '#0B0F19',
          cardDark: '#131B2E',
          cardBorderDark: '#1E293B',
          accent: '#EC4899',
          gold: '#F59E0B',
          cyan: '#06B6D4',
          emerald: '#10B981',
          violet: '#8B5CF6',

          // Backgrounds and structural surfaces
          bg: '#0B0F19',
          surface: '#131B2E',
          panel: '#1E293B',
          glass: 'rgba(255, 255, 255, 0.06)',
          glassHover: 'rgba(255, 255, 255, 0.12)',
          border: '#1E293B',
          borderSubtle: '#131B2E',

          // Brand Magenta & Crimson highlights
          crimson: '#EC4899',
          crimsonHover: '#db2777',
          crimsonBright: '#EC4899',
          crimsonGlow: '#EC4899',

          // Typography
          ivory: '#ffffff',
          textMuted: '#94a3b8',

          // Semantic accents
          ice: '#06B6D4',
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
