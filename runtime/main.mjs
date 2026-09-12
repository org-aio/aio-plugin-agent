import { createBridge } from "./bridge.mjs";
import { runSession } from "./session.mjs";

const bridge = createBridge((request) => runSession(request, bridge));
