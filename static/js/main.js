document.addEventListener('DOMContentLoaded', () => {
    // Handle ">>" quote links
    document.querySelectorAll('.post-body').forEach(body => {
        body.innerHTML = body.innerHTML.replace(
            /&gt;&gt;(\d+)/g, 
            '<a href="#p$1" class="quotelink">>>$1</a>'
        );
    });

    console.log("Rusty-Board UI initialized.");
});