/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        primary: '#16a34a', /* Backend Green */
        secondary: '#2563eb', /* UI Blue (now used for status/tabs) */
        ai: '#dc2626', /* AI Red */
        actionblue: '#007aff', /* Classic iOS/macOS Action Blue */
        actionhover: '#0064d3',
      },
      fontFamily: {
        sans: ['Inter', 'sans-serif'],
      },
    },
  },
  plugins: [],
}
