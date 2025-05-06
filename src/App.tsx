import { useState, useEffect, useRef } from "react";
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
