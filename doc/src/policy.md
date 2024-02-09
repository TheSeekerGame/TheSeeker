# Development Policies

This chapter is to describe processes and policies that everyone on the game's
dev team should follow.

Whenever we come up with conventions for how to do things, we should document
them here, so everyone is up to speed.

## Areas of Responsibility for the GitHub repository

Documentation: everyone! Feel free to contribute to docs at any time.

Assets: @c12. They are responsible for making sure all the assets committed into
the repo are valid, in the correct formats, follow our practices, etc. They should
work together with artists to put their work into the repo and ensure that.

Code:
 - @odecay:
   - Player gameplay mechanics: movement, physics, attacks, abilities, etc.
   - Enemy gameplay mechanics: AI, movement, physics, attacks, abilities, etc.
 - @destravous:
   - Shaders and graphics
 - @inodentry (Ida):
   - Everything else unless specified otherwise.

## Pull Requests and Collaboration for Programmers

We are a small team of a few developers, and as such it is manageable to have
very loose policies to keep development momentum going. :)

We use GitHub Pull Requests as an aid in development, but not a strict requirement.

For contributors that do not have push/merge permissions on GitHub, the PR process
is mandatory. Whenever you work on something, just make a PR so we can review and
merge it for you. :)

For members who have push/merge permissions on GitHub, feel free to skip the PR process
and push directly to the `main` branch **if**:
 - You are working on documentation.
 - You have made a small change that doesn't significantly change the codebase and
   does not intefere with anything others are working on (to your knowledge).
 - You have made an urgent or important fix, or you know others on the team need
   something (uncontroversial) urgently.
 - You are working on something very self-contained that is in a part of the codebase
   that only you are working on / responsible for.

For all other situations, you should create PRs.

Please use the GitHub PR Draft status to indicate if you consider your work ready for
merging. Feel free to think of Draft PRs as a "safe space" for you to work on stuff
without interfering with development on the main branch. Creating PRs for your work,
even if it is an early unfinished WIP, is good for transparency. It allows others
to see what you have been up to and give feedback.

When you change your Draft PR to a normal PR, you signal that you want your work merged.

Feel free to ask other developers on the team for code reviews at any time.

For merging PRs, anyone on the dev team with GitHub merge/push permissions can merge
PRs from other people, **if**:
 - The PR is not a Draft PR.
 - The PR is not known to interfere with any other current work outside of the author's
   area of responsibility.

You may only merge your own PR **if**:
 - All of the above.
 - You are reasonably sure that nobody on the team would object to it.
 - You have informed other team members that you are going to do it.

Reviews are optional, but we encourage everyone to solicit reviews from other team
members.
