import {
  meetingsAddSpeaker,
  meetingsMergeSpeakers,
  meetingsRenameSpeaker,
  meetingsSaveVoice,
  meetingsSetLineSpeaker,
  type Voice,
} from "../../lib/tauri/meetingsCommands";
import { useMeetingsStore } from "../../state/meetingsStore";
import { pushErrorToast, useToastStore } from "../../state/toastStore";
import { translate } from "../../state/languageStore";

/**
 * What can be done to the people of a recording, each followed by a fresh read of it: the
 * transcript, the chips and the waveform's colours all come from the same detail.
 */
async function run(meetingId: string, action: () => Promise<unknown>): Promise<boolean> {
  try {
    await action();
    await useMeetingsStore.getState().loadDetail(meetingId);
    return true;
  } catch (error) {
    pushErrorToast(String(error));
    return false;
  }
}

export const renameSpeaker = (meetingId: string, key: string, name: string) => run(meetingId, () => meetingsRenameSpeaker(meetingId, key, name));

/** Who said these lines. */
export const assignLines = (meetingId: string, seqs: number[], speaker: string) =>
  run(meetingId, async () => {
    for (const seq of seqs) await meetingsSetLineSpeaker(meetingId, seq, speaker);
  });

/** These lines were somebody not on the list yet: a new person, who gets them. */
export const assignLinesToNew = (meetingId: string, seqs: number[]) =>
  run(meetingId, async () => {
    const key = await meetingsAddSpeaker(meetingId);
    for (const seq of seqs) await meetingsSetLineSpeaker(meetingId, seq, key);
  });

/** Two people that are one: everything `from` said becomes `into`'s. */
export const mergeSpeakers = (meetingId: string, from: string, into: string) => run(meetingId, () => meetingsMergeSpeakers(meetingId, from, into));

/** Keeps a person's voice, so later recordings put the name on it by themselves. */
export const rememberVoice = (meetingId: string, key: string, name: string, isMe: boolean, voice?: Voice) =>
  run(meetingId, async () => {
    await meetingsSaveVoice({ meetingId, key, name: voice?.name ?? name, isMe: voice?.isMe ?? isMe, voiceId: voice?.id });
    useToastStore.getState().pushToast(translate("meetings.voiceSaved", { name: voice?.name ?? name }), "success");
  });
