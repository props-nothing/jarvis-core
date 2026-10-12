# Phone: JARVIS rings you, and you ring JARVIS

JARVIS can ring your phone when it needs you ("call me if the deploy fails"), and you can ring it. An **ElevenLabs agent** does the talking and listening; the tools, the memory and the rules stay with JARVIS. When the agent wants to do something, it uses JARVIS's tools through **Connect apps**, so everything it does passes the same permissions and approvals as anything else.

## Set it up (once)

1. At **elevenlabs.io**, create an agent and **import a phone number** for it (a Twilio number, or a SIP trunk). Assign the agent to the number.
2. In the agent, add JARVIS as an **MCP server**: the address and key are in **Settings, Connect apps**. For tool approval choose *Always Ask* or *Fine-Grained* so the agent asks you out loud before it acts.
3. Give the agent a prompt that says who it is and what it may do. A start:
   > You are JARVIS, the owner's AI assistant, speaking on the phone. Say you are an AI. If you were asked to call, say why: {{jarvis_reason}}. Use your tools to help; ask before anything that changes something.
4. In JARVIS open **Settings, Voice** and save your ElevenLabs key. It must be allowed to use the agents platform, not only speech.
5. Open **Settings, Phone**. Fill in the **agent id**, the **phone number id** (the page lists your numbers and their ids once the key is set) and **your own number** in international format (`+31612345678`). JARVIS rings only the numbers you list; separate several with commas.
6. Press **Call me now**. Your phone rings; answer it.

These settings apply at once, with no restart. `jarvis config set phone_owner_numbers "+31612345678"` does the same from a terminal.

## Ringing JARVIS

Call the number you imported. The agent answers and uses JARVIS's tools. JARVIS does not check who is calling: a phone number is not proof of who is speaking. Whatever the agent may do for a caller is what you allowed in its prompt and in JARVIS's permissions, and anything that needs your approval waits for you as usual.

## JARVIS ringing you

JARVIS gets two tools: *call the owner* and *how did the call go*. Asking JARVIS "call me when this is done" makes it place the call when the time comes.

- **It asks you first**, every time, like any message to another person. In **Settings, Permissions** you can set *Run without asking* for `jarvis.voice.call`, so a scheduled task or trigger can ring you on its own. That is your choice, per tool.
- **It rings only a number you listed**, never one a model makes up.
- **Not during quiet hours** (`quiet_hours` under Advanced), unless JARVIS says it cannot wait.
- **At most 3 calls an hour and 10 a day**, and one at a time. A call that fails is not tried again.
- The agent is told the reason as the variable `jarvis_reason`. If you turn on **JARVIS writes the opening line**, JARVIS also sets the agent's first sentence ("Hello, this is JARVIS, your AI assistant. I am calling because: ..."); that needs the agent's *First message* override switched on in its **Security** tab at ElevenLabs.

## One yes, not two

Without any setting, a tool the phone agent asks for waits for your click in the console (or Telegram) even if the agent already asked you out loud. If you would rather answer once, on the phone, switch on **Your yes on the phone counts**. It applies only:

- while a call **JARVIS placed to your number** has been answered (not while it rings), and for 30 minutes at most;
- to tools of risk 2 or less (adding a schedule, a note; not writing files or sending mail);
- never to the phone tools themselves, and never to a tool you set to *Always ask first*.

JARVIS cannot see whether the agent really asked you. With this on and the agent set to *No Approval* at ElevenLabs, the agent can use those tools without asking anyone while you are on the call, and whoever answers your phone is treated as you. Set the agent to ask first.

## What is kept

JARVIS keeps a short record of each call it placed: the number (shown masked), the reason, when, how long, how it ended and the provider's one-paragraph summary. It does **not** keep the transcript or the audio. **Delete call records** in Settings clears the list; records older than 90 days are removed when the daemon starts. The call, its transcript and any audio pass through ElevenLabs and your telephone provider and are kept there under your account's settings.

## What it does not do yet

JARVIS cannot hang up a call, does not recognise who rings it, and learns how a call went by asking ElevenLabs every ten seconds, not from a webhook. Accounts with data residency (an EU, India or Singapore ElevenLabs host) are not supported yet. The first real call with your account is the check that matters: if **Call me now** fails, the message says why (a key without access to agents, an id that is not in your account, an override that is not enabled).
