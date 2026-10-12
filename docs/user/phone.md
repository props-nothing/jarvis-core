# Phone: JARVIS rings you, and you ring JARVIS

JARVIS can ring your phone when it needs you ("call me if the deploy fails"), and you can ring it. An **ElevenLabs agent** does the talking and listening; the tools, the memory and the rules stay with JARVIS. When the agent wants to do something, it uses JARVIS's tools through **Connect apps**, so everything it does passes the same permissions and approvals as anything else.

## Set it up (once)

1. At **elevenlabs.io**, create an agent and **import a phone number** for it (a Twilio number, or a SIP trunk). Assign the agent to the number.
2. In the agent, add JARVIS as an **MCP server**: the address and key are in **Settings, Connect apps**. For tool approval choose *Always Ask* or *Fine-Grained* so the agent asks you out loud before it acts.
3. Give the agent a prompt that says who it is and that it is on the phone. A start (no custom `{{variables}}`, see below):
   > You are JARVIS, the owner's AI assistant, speaking on the phone. Say that you are an AI. Keep answers short and spoken: no lists, no markdown. If you opened the call, the first thing you said was the reason. Use your tools to help; ask before anything that changes something.
4. In JARVIS open **Settings, Voice** and save your ElevenLabs key. It must be allowed to use the agents platform, not only speech.
5. Open **Settings, Phone**. Fill in the **agent id**, the **phone number id** (the page lists your numbers and their ids once the key is set) and **your own number** in international format (`+31612345678`). JARVIS rings only the numbers you list; separate several with commas.
6. Press **Call me now**. Your phone rings; answer it.

These settings apply at once, with no restart. `jarvis config set phone_owner_numbers "+31612345678"` does the same from a terminal.

## Ringing JARVIS

Call the number you imported. The agent answers and uses JARVIS's tools. **Only your own number gets any use of JARVIS**, in two layers (`ADR-0201`).

**The door (switch on "Answer only my own numbers", or `jarvis config set phone_screen_callers on`).** Before an inbound call starts, ElevenLabs asks JARVIS who is calling, and JARVIS says yes only to the numbers you listed. For anyone else the call never starts and they get no agent at all. JARVIS registers this with ElevenLabs by itself and follows its internet address when it changes (it needs *Reachable from the internet* in Connect apps). Settings, Phone shows whether the door is guarding and lists the callers it turned away. While JARVIS is off or its address is down, **nobody gets an answer, you included**: that is the price of a door that fails closed.

**The line guard (always on).** While any other call is open on the agent, JARVIS refuses every tool for every app. It catches what the door cannot: a call that started without asking (a chat in a browser, or before the door was switched on).

Limits that remain:

- Without the door switched on, a stranger can still ring the number and talk to the agent (and use up ElevenLabs credit); the line guard only makes the call useless to them.
- Caller ID can be forged: someone who fakes your number is you, as far as either layer goes. Keep the agent's MCP tools on *Always Ask* at ElevenLabs and your acting tools on *Always ask first* in Settings, Permissions.
- While a stranger's call is open (or one JARVIS placed to someone else), your other apps such as Claude Code are refused too, for as long as the call lasts. A chat you start yourself in the ElevenLabs dashboard has no number, so it is held back as well.
- In the agent's **Security** tab, switch on authentication, or anyone who knows the agent's id can start a web conversation with it.

**If you ring and nothing happens**, the call is being stopped before a conversation exists. Look at the *Calls history* in ElevenLabs: no new row means it never got that far. Check, in this order:

1. **The door.** With "Answer only my own numbers" on, an unreachable JARVIS means no answer for anyone. Settings, Phone says whether the door is guarding or what is wrong (for example that JARVIS has no internet address yet). If the door is **off**, the agent's Security tab setting "Fetch initiation client data from a webhook" must be off too, because ElevenLabs documents that "a failed or timed-out webhook can prevent the conversation from starting": with it on and a dead address, every inbound call dies silently while calls JARVIS places still work (found 2026-10-12 on the owner's agent, whose workspace pointed at an old development tunnel).
2. **The number must be a purchased Twilio number**, not a verified caller ID (those are outbound only). "Check with ElevenLabs" in Settings, Phone, shows the number; ElevenLabs reports `supports_inbound`.
3. **The number must be assigned to the agent** (the same check shows which agent).
4. **In the Twilio console**, the number's voice configuration should point to ElevenLabs (it does after import). A Twilio *trial* account plays a trial message first and can need a key press; geographic permissions (Voice, Geo permissions) must allow calls from your country; some countries need a regulatory bundle before a number can receive calls.
5. **The agent's tools need an MCP address that is alive.** The quick-tunnel address changes every time JARVIS starts, so the MCP server you added in ElevenLabs stops working after a restart (its address cannot be edited; add a new MCP server and attach it). The fix is your own Cloudflare tunnel: an address that stays the same, set up once under Settings, Connect apps, From the internet (steps in [Connect apps](connect-apps.md#an-address-that-never-changes-your-own-cloudflare-tunnel)). Without it, restart JARVIS as rarely as you can.

Do not put `{{jarvis_reason}}` or another custom variable in the agent's prompt: an inbound call does not supply it and ElevenLabs refuses a conversation with an undefined variable. For the reason of a call JARVIS places, turn on **JARVIS writes the opening line** instead (the agent needs its *First message* override on, which it has).

## JARVIS ringing other people

Switch on **JARVIS may call other people** (Settings, Phone, or `jarvis config set phone_call_anyone on`) and JARVIS gets a third tool, *call a number*, for things like "call the restaurant and book a table for two". It is a separate tool so that letting JARVIS ring *you* unasked never lets it ring anyone else unasked.

- **It asks you first, every time**, and shows you the number and what it will say. Leave it that way. Only trust it by name if you accept that a hostile web page or mail could steer JARVIS into ringing someone.
- **The person who answers is told first**: "Hello, this is JARVIS, an AI assistant calling on behalf of my owner. I am calling to: ..." JARVIS writes that sentence itself, whatever the opening-line setting says. The agent's *First message* override must be switched on at ElevenLabs (Security tab), or ElevenLabs refuses the call; JARVIS never rings a stranger without the sentence.
- **Never in quiet hours**, never a premium-rate number, never your own number (use the other tool), at most 5 such calls a day and 2 to the same number a day.
- **The agent can only talk on such a call**: while it is open, none of JARVIS's tools work for it. Put what the call is for in the purpose.
- Afterwards ask how it went; JARVIS reads the summary and what was said, as text to read and never as instructions.

## JARVIS ringing you

JARVIS gets two tools: *call the owner* and *how did the call go*. Asking JARVIS "call me when this is done" makes it place the call when the time comes.

- **It asks you first**, every time, like any message to another person. In **Settings, Permissions** you can set *Run without asking* for `jarvis.voice.call`, so a scheduled task or trigger can ring you on its own. That is your choice, per tool.
- **It rings only a number you listed**, never one a model makes up.
- **Not during quiet hours** (`quiet_hours` under Advanced), unless JARVIS says it cannot wait.
- **At most 3 calls an hour and 10 a day**, and one at a time. A call that fails is not tried again.
- Whatever the agent is told about the call, it is told in a **brief** JARVIS sends with the call: your agent's own prompt, then what the call is for, with rules to stay on that, never invent offers or facts, and never claim something was done that no tool confirmed. This needs the agent to allow a prompt per call, which JARVIS switches on once by itself. A call to someone else is not placed if the brief cannot be delivered.
- The reason is also sent as the variable `jarvis_reason`, but an agent only sees it if its prompt uses it (which breaks inbound calls, see above), so on its own the agent does not know why it was asked to call. Two ways to tell it, both needing the agent's *First message* override switched on in its **Security** tab at ElevenLabs: the assistant can give the **exact first sentence** (`opening`, spoken as written, so it says it is an AI only if the assistant writes that), or, with **JARVIS writes the opening line** on, JARVIS opens with "Hello, this is JARVIS. I am calling because: ...". The agent never says it is an AI by itself: its own prompt and first message ("Hello Sir, This is JARVIS.") are set at ElevenLabs. **Calls to other people are different**: they always open by saying an AI is calling for you, and nothing can switch that off.

## One yes, not two

Without any setting, a tool the phone agent asks for waits for your click in the console (or Telegram) even if the agent already asked you out loud. If you would rather answer once, on the phone, switch on **Your yes on the phone counts**. It applies only:

- while a call **JARVIS placed to your number** has been answered (not while it rings), and for 30 minutes at most;
- to tools of risk 2 or less (adding a schedule, a note; not writing files or sending mail);
- never to the phone tools themselves, and never to a tool you set to *Always ask first*.

JARVIS cannot see whether the agent really asked you. With this on and the agent set to *No Approval* at ElevenLabs, the agent can use those tools without asking anyone while you are on the call, and whoever answers your phone is treated as you. Set the agent to ask first.

## What is kept

JARVIS keeps a short record of each call it placed: the number (shown masked), the reason, when, how long, how it ended and the provider's one-paragraph summary. It does **not** keep the transcript or the audio. **Delete call records** in Settings clears the list; records older than 90 days are removed when the daemon starts. The call, its transcript and any audio pass through ElevenLabs and your telephone provider and are kept there under your account's settings.

## What it does not do yet

JARVIS cannot hang up a call or turn a stranger's call away at the door (see above), and learns how a call went by asking ElevenLabs every ten seconds, not from a webhook. Accounts with data residency (an EU, India or Singapore ElevenLabs host) are not supported yet. The first real call with your account is the check that matters: if **Call me now** fails, the message says why (a key without access to agents, an id that is not in your account, an override that is not enabled).
