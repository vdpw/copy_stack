import { useState, useEffect, useRef } from "react";
import reactLogo from "./assets/react.svg";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { listen } from "@tauri-apps/api/event";


async function startListening(on_msg: React.Dispatch<React.SetStateAction<string>>) {
  try {
    await invoke("start_listening");
    listen("message", (payload) => {
      on_msg(payload.payload as string);
    });
  } catch (error) {
    console.error(error);
  }
}

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const listenerSet = useRef(false);
  useEffect(() => {
    if (!listenerSet.current) {
      startListening(setGreetMsg);
      listenerSet.current = true;
    }
  }, []);

  return (
    <main className="container">
      <h1>Welcome to Tauri + React</h1>

      <div className="row">
        <a href="https://vitejs.dev" target="_blank">
          <img src="/vite.svg" className="logo vite" alt="Vite logo" />
        </a>
        <a href="https://tauri.app" target="_blank">
          <img src="/tauri.svg" className="logo tauri" alt="Tauri logo" />
        </a>
        <a href="https://reactjs.org" target="_blank">
          <img src={reactLogo} className="logo react" alt="React logo" />
        </a>
      </div>
      <p>Click on the Tauri, Vite, and React logos to learn more.</p>

      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
        }}
      >
        <input
          id="greet-input"
          onChange={() => {}}
          placeholder="Enter a name..."
        />
        <button type="submit">Greet</button>
      </form>
      <p>{greetMsg}</p>
    </main>
  );
}

export default App;
