/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        background: "#0f172a",
        surface: "#1e293b",
        border: "#334155",
        primary: "#38bdf8",
      },
    },
  },
  plugins: [],
};
